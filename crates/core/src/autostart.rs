//! Launch-at-login (`app.setLoginItemSettings` in the original).
//!
//! Implemented with the per-user `Run` registry key.

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "ClipAssistant";
/// Value name written by the previous release of this rewrite. It is removed on
/// start-up so that the old entry and the new one do not both launch the app.
const LEGACY_VALUE_NAME: &str = "剪切助手";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Open the Run key, creating it when `write` is requested.
fn open_key(write: bool) -> Option<HKEY> {
    unsafe {
        let sub = wide(RUN_KEY);
        let mut hkey = HKEY::default();
        let access = if write { KEY_WRITE } else { KEY_READ };
        let rc = RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), None, access, &mut hkey);
        if rc.0 == 0 {
            return Some(hkey);
        }
        if !write {
            return None;
        }
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if rc.0 != 0 {
            return None;
        }
        Some(hkey)
    }
}

/// Absolute path of the running executable.
pub fn current_exe() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Drop the legacy `剪切助手` entry, but only when it still points at the
/// previous binary — another application's entry with that name is left alone.
fn remove_legacy_entry(hkey: HKEY) {
    unsafe {
        let name = wide(LEGACY_VALUE_NAME);
        let mut size = 0u32;
        if RegQueryValueExW(hkey, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size)).0 != 0
            || size == 0
        {
            return;
        }
        let mut buf = vec![0u8; size as usize];
        let mut kind = REG_SZ;
        if RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
        .0 != 0
        {
            return;
        }
        let data = String::from_utf16_lossy(std::slice::from_raw_parts(
            buf.as_ptr() as *const u16,
            size as usize / 2,
        ));
        if data.to_ascii_lowercase().contains("jianqiezhushou") {
            let _ = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
        }
    }
}

/// Enable or disable launching at login.
pub fn set_enabled(enabled: bool) -> bool {
    unsafe {
        let Some(hkey) = open_key(true) else {
            return false;
        };
        remove_legacy_entry(hkey);
        let name = wide(VALUE_NAME);
        let ok = if enabled {
            let exe = wide(&format!("\"{}\"", current_exe()));
            let bytes = std::slice::from_raw_parts(exe.as_ptr() as *const u8, exe.len() * 2);
            RegSetValueExW(hkey, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)).0 == 0
        } else {
            // Removing a value that is not present still counts as success.
            RegDeleteValueW(hkey, PCWSTR(name.as_ptr())).0 == 0 || !is_enabled()
        };
        let _ = RegCloseKey(hkey);
        ok
    }
}

/// Whether the Run entry is currently present.
pub fn is_enabled() -> bool {
    unsafe {
        let Some(hkey) = open_key(false) else {
            return false;
        };
        let name = wide(VALUE_NAME);
        let mut size = 0u32;
        let rc = RegQueryValueExW(hkey, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size));
        let _ = RegCloseKey(hkey);
        rc.0 == 0 && size > 0
    }
}
