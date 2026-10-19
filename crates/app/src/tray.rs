//! System tray icon and its context menu.
//!
//! Replaces Electron's `Tray` + `Menu`. The icon is generated at runtime so no
//! image asset is needed.

use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, PostMessageW, RegisterClassW, SetForegroundWindow,
    TrackPopupMenu, TranslateMessage, CreateIconIndirect, ICONINFO,
    MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND,
    WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;

const CMD_OPEN: usize = 1;
const CMD_SETTINGS: usize = 2;
const CMD_PREV: usize = 3;
const CMD_NEXT: usize = 4;
const CMD_GUIDE: usize = 5;
const CMD_QUIT: usize = 6;

/// Events raised by the tray, drained by the UI event pump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    TogglePanel,
    OpenSettings,
    PreviousGroup,
    NextGroup,
    Guide,
    Quit,
}

static EVENTS: Mutex<Vec<TrayEvent>> = Mutex::new(Vec::new());
static HWND_SLOT: Mutex<isize> = Mutex::new(0);

/// Take any pending tray events.
pub fn drain() -> Vec<TrayEvent> {
    std::mem::take(&mut *EVENTS.lock().unwrap())
}

pub fn push(event: TrayEvent) {
    EVENTS.lock().unwrap().push(event);
}

/// Show or hide the tray icon.
pub fn set_visible(visible: bool) {
    let hwnd = HWND(*HWND_SLOT.lock().unwrap() as *mut _);
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let mut nid = notify_data(hwnd, Default::default());
        if visible {
            let _ = Shell_NotifyIconW(NIM_ADD, &mut nid);
        } else {
            let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
        }
    }
}

/// Spawn the tray thread. Returns immediately.
pub fn start() {
    std::thread::spawn(|| unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            return;
        };
        let class: Vec<u16> = "clipassistant_tray".encode_utf16().chain(std::iter::once(0)).collect();
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(tray_proc);
        wc.hInstance = HINSTANCE(hinstance.0);
        wc.lpszClassName = PCWSTR(class.as_ptr());
        let _ = RegisterClassW(&wc);

        let Ok(hwnd) = CreateWindowExW(
            Default::default(),
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(HINSTANCE(hinstance.0)),
            None,
        ) else {
            return;
        };
        *HWND_SLOT.lock().unwrap() = hwnd.0 as isize;

        let mut nid = notify_data(hwnd, icon_handle());
        let _ = Shell_NotifyIconW(NIM_ADD, &mut nid);

        let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = DestroyWindow(hwnd);
    });
}

fn notify_data(hwnd: HWND, icon: windows::Win32::UI::WindowsAndMessaging::HICON) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ID;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    let tip: Vec<u16> = clipassistant_core::APP_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let n = tip.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    nid
}

extern "system" fn tray_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TRAY => {
                let event = lparam.0 as u32;
                if event == WM_LBUTTONUP {
                    push(TrayEvent::TogglePanel);
                } else if event == WM_RBUTTONUP {
                    show_menu(hwnd);
                }
            }
            WM_COMMAND => {
                match wparam.0 & 0xFFFF {
                    CMD_OPEN => push(TrayEvent::TogglePanel),
                    CMD_SETTINGS => push(TrayEvent::OpenSettings),
                    CMD_PREV => push(TrayEvent::PreviousGroup),
                    CMD_NEXT => push(TrayEvent::NextGroup),
                    CMD_GUIDE => push(TrayEvent::Guide),
                    CMD_QUIT => push(TrayEvent::Quit),
                    _ => {}
                }
            }
            WM_DESTROY => {
                let mut nid = notify_data(hwnd, Default::default());
                let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else { return };
    let item = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let open_label = format!("打开{}", clipassistant_core::APP_NAME);
    let entries: [(usize, &str); 7] = [
        (CMD_SETTINGS, "偏好设置"),
        (CMD_OPEN, &open_label),
        (CMD_PREV, "上一个分组"),
        (CMD_NEXT, "下一个分组"),
        (0, ""),
        (CMD_GUIDE, "使用指南"),
        (CMD_QUIT, "退出"),
    ];
    for (id, label) in entries {
        let text = item(label);
        if id == 0 {
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        } else {
            let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(text.as_ptr()));
        }
    }
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        None,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
    let _ = PostMessageW(Some(hwnd), 0, WPARAM(0), LPARAM(0));
}

/// Build the tray icon: a rounded accent square with a clipboard glyph.
fn icon_handle() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    const SIZE: i32 = 32;
    unsafe {
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIZE,
            biHeight: -SIZE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let Ok(color) = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            return Default::default();
        };
        if bits.is_null() {
            return Default::default();
        }
        let px = std::slice::from_raw_parts_mut(bits as *mut u8, (SIZE * SIZE * 4) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = ((y * SIZE + x) * 4) as usize;
                let dx = (x as f32 - 15.5).abs();
                let dy = (y as f32 - 15.5).abs();
                let inside = dx < 15.0 && dy < 15.0 && (dx - 9.0).max(dy - 9.0).max(0.0) < 6.0;
                // Accent #726CFF background, white clipboard body in the middle.
                let body = (5..27).contains(&x) && (4..28).contains(&y) && !(x > 9 && x < 22 && y > 7 && y < 10);
                let (b, g, r) = if body && (x > 7 && x < 24 && y > 10 && y < 25) {
                    (255u8, 255u8, 255u8)
                } else if inside {
                    (255u8, 108u8, 114u8) // BGRA of #726CFF
                } else {
                    (0u8, 0u8, 0u8)
                };
                let alpha = if inside && body {
                    255
                } else if inside {
                    235
                } else {
                    0
                };
                px[i] = b;
                px[i + 1] = g;
                px[i + 2] = r;
                px[i + 3] = alpha;
            }
        }
        let mask_bits = vec![0u8; (SIZE * SIZE) as usize / 8];
        let mask: HBITMAP = CreateBitmap(SIZE, SIZE, 1, 1, Some(mask_bits.as_ptr() as *const _));
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(HGDIOBJ(mask.0));
        let _ = DeleteObject(HGDIOBJ(color.0));
        icon
    }
}
