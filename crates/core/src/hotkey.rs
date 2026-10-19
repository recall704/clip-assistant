//! Global hotkeys.
//!
//! The original used Electron's `globalShortcut`. We register native Win32
//! hotkeys against a message-only window owned by the UI thread, so the
//! activations arrive through GPUI's own message pump.

use crate::winutil::{self, MessageWindow};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use windows::Win32::Foundation::HWND;

/// `WM_HOTKEY`
const WM_HOTKEY: u32 = 0x0312;

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
pub const MOD_NOREPEAT: u32 = 0x4000;

enum Cmd {
    Register(i32, u32, u32),
    Unregister(i32),
    Clear,
}

/// Register hotkeys and receive their activations.
pub struct HotkeyManager {
    tx: Sender<Cmd>,
    win: MessageWindow,
    /// Set to `false` to ignore hotkey presses while the panel is animating.
    pub enabled: Arc<Mutex<bool>>,
}

impl HotkeyManager {
    /// Spawn the hotkey thread. `on_hotkey` is invoked with the id of the
    /// hotkey that fired.
    pub fn new(on_hotkey: impl Fn(i32) + Send + Sync + 'static) -> Option<Arc<HotkeyManager>> {
        let (tx, rx) = channel::<Cmd>();
        let callback: Arc<dyn Fn(i32) + Send + Sync> = Arc::new(on_hotkey);
        let cb = callback.clone();
        let enabled = Arc::new(Mutex::new(true));
        let enabled_for_handler = enabled.clone();
        // The window handle is needed inside its own window procedure, so it is
        // published right after creation.
        let hwnd_cell: Arc<OnceLock<isize>> = Arc::new(OnceLock::new());
        let cell = hwnd_cell.clone();

        let win = MessageWindow::new("clipassistant_hotkey_window", move |msg, wparam, _lparam| {
            match msg {
                WM_HOTKEY => {
                    if *enabled_for_handler.lock().unwrap() {
                        cb(wparam.0 as i32);
                    }
                }
                m if m == winutil::WM_APP_INVOKE => {
                    let owner = cell.get().copied().unwrap_or(0);
                    drain(&rx, owner);
                }
                _ => {}
            }
        })?;
        let _ = hwnd_cell.set(win.hwnd.0 as isize);

        Some(Arc::new(HotkeyManager { tx, win, enabled }))
    }

    /// Register `keystroke` under `id`, replacing any previous binding.
    pub fn register(&self, id: i32, keystroke: &str) -> bool {
        let Some((mods, vk)) = parse_keystroke(keystroke) else {
            log::warn!("无法解析快捷键: {keystroke}");
            return false;
        };
        self.unregister(id);
        let _ = self.tx.send(Cmd::Register(id, mods, vk));
        self.win.post();
        true
    }

    pub fn unregister(&self, id: i32) {
        let _ = self.tx.send(Cmd::Unregister(id));
        self.win.post();
    }

    pub fn clear(&self) {
        let _ = self.tx.send(Cmd::Clear);
        self.win.post();
    }

    /// Temporarily ignore hotkey activations (e.g. while pasting).
    pub fn set_enabled(&self, on: bool) {
        *self.enabled.lock().unwrap() = on;
    }
}

fn drain(rx: &Receiver<Cmd>, owner: isize) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    };
    // The hotkeys belong to `owner` so that WM_HOTKEY is delivered to that
    // window (a `None` owner would post it to the thread queue instead).
    let hwnd = if owner == 0 {
        None
    } else {
        Some(HWND(owner as *mut _))
    };
    let mut registered: HashMap<i32, ()> = HashMap::new();
    while let Ok(cmd) = rx.try_recv() {
        unsafe {
            match cmd {
                Cmd::Register(id, mods, vk) => {
                    const MOD_NOREPEAT_V: u32 = 0x4000;
                    match RegisterHotKey(hwnd, id, HOT_KEY_MODIFIERS(mods | MOD_NOREPEAT_V), vk) {
                        Ok(()) => {
                            registered.insert(id, ());
                        }
                        Err(err) => {
                            log::warn!("注册快捷键失败 id={id} mods=0x{mods:X} vk=0x{vk:X} ({err})");
                        }
                    }
                }
                Cmd::Unregister(id) => {
                    let _ = UnregisterHotKey(hwnd, id);
                }
                Cmd::Clear => {
                    for id in registered.keys() {
                        let _ = UnregisterHotKey(hwnd, *id);
                    }
                    registered.clear();
                }
            }
        }
    }
}

/// Hotkey ids used by the application.
pub mod ids {
    pub const SHOW_HIDE: i32 = 1;
    pub const PREV_GROUP: i32 = 2;
    pub const NEXT_GROUP: i32 = 3;
    pub const QUICK_PASTE_1: i32 = 101;
    pub const QUICK_PASTE_9: i32 = 109;
    pub const APPOINT_BASE: i32 = 1000;
}

/// Parse `ctrl+shift+a` into `(MOD_*, VK)`.
pub fn parse_keystroke(s: &str) -> Option<(u32, u32)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut mods = 0u32;
    let mut key: Option<u32> = None;
    for part in s.split('+') {
        let p = part.trim().to_ascii_lowercase();
        match p.as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "command" | "cmd" | "super" | "win" | "meta" => mods |= MOD_WIN,
            "alt" | "option" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "" => {}
            other => {
                if key.is_some() {
                    return None;
                }
                key = Some(vk_for(other)?);
            }
        }
    }
    key.map(|vk| (mods, vk))
}

fn vk_for(key: &str) -> Option<u32> {
    use windows::Win32::UI::Input::KeyboardAndMouse as k;
    let single = key.chars().next()?;
    if key.len() == 1 {
        return Some(match single {
            'a'..='z' => single.to_ascii_uppercase() as u32,
            '0'..='9' => single as u32,
            '`' | '~' => k::VK_OEM_3.0 as u32,
            '-' | '_' => k::VK_OEM_MINUS.0 as u32,
            '=' | '+' => k::VK_OEM_PLUS.0 as u32,
            '[' | '{' => k::VK_OEM_4.0 as u32,
            ']' | '}' => k::VK_OEM_6.0 as u32,
            '\\' | '|' => k::VK_OEM_5.0 as u32,
            ';' | ':' => k::VK_OEM_1.0 as u32,
            '\'' | '"' => k::VK_OEM_7.0 as u32,
            ',' | '<' => k::VK_OEM_COMMA.0 as u32,
            '.' | '>' => k::VK_OEM_PERIOD.0 as u32,
            '/' | '?' => k::VK_OEM_2.0 as u32,
            ' ' => k::VK_SPACE.0 as u32,
            _ => return None,
        });
    }
    Some(match key {
        "space" => k::VK_SPACE.0 as u32,
        "tab" => k::VK_TAB.0 as u32,
        "enter" | "return" => k::VK_RETURN.0 as u32,
        "escape" | "esc" => k::VK_ESCAPE.0 as u32,
        "backspace" => k::VK_BACK.0 as u32,
        "delete" | "del" => k::VK_DELETE.0 as u32,
        "insert" => k::VK_INSERT.0 as u32,
        "home" => k::VK_HOME.0 as u32,
        "end" => k::VK_END.0 as u32,
        "pageup" => k::VK_PRIOR.0 as u32,
        "pagedown" => k::VK_NEXT.0 as u32,
        "up" => k::VK_UP.0 as u32,
        "down" => k::VK_DOWN.0 as u32,
        "left" => k::VK_LEFT.0 as u32,
        "right" => k::VK_RIGHT.0 as u32,
        "f1" => k::VK_F1.0 as u32,
        "f2" => k::VK_F2.0 as u32,
        "f3" => k::VK_F3.0 as u32,
        "f4" => k::VK_F4.0 as u32,
        "f5" => k::VK_F5.0 as u32,
        "f6" => k::VK_F6.0 as u32,
        "f7" => k::VK_F7.0 as u32,
        "f8" => k::VK_F8.0 as u32,
        "f9" => k::VK_F9.0 as u32,
        "f10" => k::VK_F10.0 as u32,
        "f11" => k::VK_F11.0 as u32,
        "f12" => k::VK_F12.0 as u32,
        _ => return None,
    })
}

/// Human readable name for a key code (reverse of `vk_for`, main keys only).
pub fn key_display_name(vk: u32) -> String {
    match vk {
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDD => "]".into(),
        0x20 => "space".into(),
        0x0D => "Enter".into(),
        0x09 => "Tab".into(),
        v if (0x41..=0x5A).contains(&v) => ((v as u8) as char).to_ascii_lowercase().to_string(),
        v if (0x30..=0x39).contains(&v) => ((v as u8) as char).to_string(),
        v => format!("0x{v:X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shortcuts() {
        let (m, k) = parse_keystroke("ctrl+`").unwrap();
        assert_eq!(m, MOD_CONTROL);
        assert_eq!(k, 0xC0);
        let (m, k) = parse_keystroke("ctrl+shift+a").unwrap();
        assert_eq!(m, MOD_CONTROL | MOD_SHIFT);
        assert_eq!(k, 0x41);
        let (m, k) = parse_keystroke("command+1").unwrap();
        assert_eq!(m, MOD_WIN);
        assert_eq!(k, 0x31);
        assert!(parse_keystroke("").is_none());
        assert!(parse_keystroke("ctrl+").is_none());
    }
}
