//! Low level Win32 helpers shared by the clipboard monitor, the hotkey
//! manager and the paste automation.
//!
//! Everything in here replaces one of the original `.exe` helper programs
//! (`getHandle.exe`, `setHandle.exe`, `setClipboard.exe`, `getFolder.exe`,
//! `clipboardListener.exe`) with in-process code.

use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, VIRTUAL_KEY,
    VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PostMessageW, RegisterClassW, SetWindowLongPtrW, TranslateMessage,
    GWLP_USERDATA, HWND_MESSAGE, MSG, WNDCLASSW, WS_OVERLAPPED,
};

/// Convert a Rust string into a NUL terminated UTF-16 buffer.
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Custom message used to wake the loop thread so it can run queued closures.
pub const WM_APP_INVOKE: u32 = 0x8000 + 1;

/// A boxed closure used as a window procedure.
pub type Handler = Box<dyn Fn(u32, WPARAM, LPARAM) + Send + 'static>;

const KEYEVENTF_KEYUP: u32 = 0x0002;

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Handler;
        if !ptr.is_null() {
            // Safety: the pointer is a Box leaked in `MessageWindow::new` and
            // lives until the window is destroyed.
            let handler: &Handler = &*ptr;
            handler(msg, wparam, lparam);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// A message-only window whose window procedure is a Rust closure.
pub struct MessageWindow {
    pub hwnd: HWND,
    handler: *mut Handler,
}

unsafe impl Send for MessageWindow {}
unsafe impl Sync for MessageWindow {}

impl MessageWindow {
    /// Create a message-only window. `handler` sees every message.
    pub fn new(
        class_name: &str,
        handler: impl Fn(u32, WPARAM, LPARAM) + Send + 'static,
    ) -> Option<MessageWindow> {
        unsafe {
            let hinstance = GetModuleHandleW(None).ok()?;
            let class = to_wide(class_name);

            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(wnd_proc);
            wc.hInstance = HINSTANCE(hinstance.0);
            wc.lpszClassName = PCWSTR(class.as_ptr());
            // A non-zero return means success; a stale class from a previous
            // run is harmless.
            let _ = RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                Default::default(),
                PCWSTR(class.as_ptr()),
                PCWSTR(class.as_ptr()),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(HINSTANCE(hinstance.0)),
                None,
            )
            .ok()?;

            let inner: Handler = Box::new(handler);
            let handler_ptr: *mut Handler = Box::into_raw(Box::new(inner));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, handler_ptr as isize);
            Some(MessageWindow {
                hwnd,
                handler: handler_ptr,
            })
        }
    }

    /// Wake the loop thread (useful to run queued work).
    pub fn post(&self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_APP_INVOKE, WPARAM(0), LPARAM(0));
        }
    }

    /// Blocking message pump, run on the thread that created the window.
    pub fn run_message_loop() {
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

impl Drop for MessageWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            if !self.handler.is_null() {
                drop(Box::from_raw(self.handler));
                self.handler = std::ptr::null_mut();
            }
        }
    }
}

/// Foreground window handle (replaces `getHandle.exe`).
pub fn foreground_window() -> HWND {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }
}

/// Bring `hwnd` back to the foreground (replaces `setHandle.exe`).
pub fn set_foreground_window(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        use windows::Win32::System::Threading::AttachThreadInput;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            return true;
        }
        // Bypass the foreground lock by temporarily sharing input state.
        let fg = foreground_window();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let target_thread = GetWindowThreadProcessId(hwnd, None);
        let current = windows::Win32::System::Threading::GetCurrentThreadId();
        let _ = AttachThreadInput(current, fg_thread, true);
        let _ = AttachThreadInput(current, target_thread, true);
        let ok = SetForegroundWindow(hwnd).as_bool();
        let _ = AttachThreadInput(current, fg_thread, false);
        let _ = AttachThreadInput(current, target_thread, false);
        ok
    }
}

/// Title text of a window.
pub fn window_title(hwnd: HWND) -> String {
    unsafe {
        let mut buf = vec![0u16; 512];
        let n = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf);
        let n = n.max(0) as usize;
        String::from_utf16_lossy(&buf[..n])
    }
}

fn key_input(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: KEYBD_EVENT_FLAGS(if up { KEYEVENTF_KEYUP } else { 0 }),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Send `modifier + V` to the focused window (the original `robotjs.keyTap`).
pub fn send_paste(modifier: VIRTUAL_KEY) {
    let inputs = [
        key_input(modifier.0, false),
        key_input(VK_V.0, false),
        key_input(VK_V.0, true),
        key_input(modifier.0, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// `Ctrl+V` — the Windows paste gesture.
pub fn send_ctrl_v() {
    send_paste(VK_CONTROL);
}

/// Total size in bytes of a file or directory tree (replaces `getFolder.exe`).
pub fn path_size(path: &Path) -> i64 {
    match std::fs::metadata(path) {
        Ok(md) if md.is_file() => return md.len() as i64,
        Ok(_) => {}
        Err(_) => return -1,
    }
    let mut total: i64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if let Ok(md) = entry.metadata() {
                total += md.len() as i64;
            }
        }
    }
    total
}

pub fn is_file(path: &str) -> bool {
    std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

pub fn is_folder(path: &str) -> bool {
    std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// Current mouse position in physical pixels.
pub fn cursor_position() -> (i32, i32) {
    use windows::Win32::Foundation::POINT;
    let mut pt = POINT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

/// Open a path or URL with the shell.
pub fn open_with_shell(target: &str) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let op = to_wide("open");
    let file = to_wide(target);
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Build the JSON payload shown on a file/folder card (`utils.getPasteContent`).
pub fn file_descriptor_json(path: &str, title: &str, size: i64, kind: i64) -> String {
    let url = format!("static://{path}");
    serde_json::json!({
        "content": path,
        "url": url,
        "size": size,
        "title": title,
        "type": kind,
    })
    .to_string()
}
