//! Which application did the content come from (`active-win` in the original).
//!
//! On Windows the original only recorded the owning process name and left the
//! icon empty; we additionally extract the executable's icon so cards look
//! complete offline.

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Details stored in `post.sourceApp` as JSON.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SourceApp {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub hash: String,
}

impl SourceApp {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(s: &str) -> SourceApp {
        serde_json::from_str(s).unwrap_or_default()
    }
}

/// Executable path of the process owning `pid`.
pub fn process_image_path(pid: u32) -> Option<std::path::PathBuf> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok || len == 0 {
            return None;
        }
        Some(std::path::PathBuf::from(String::from_utf16_lossy(
            &buf[..len as usize],
        )))
    }
}

/// Name of the process that currently owns the foreground window.
pub fn active_app_name() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return "未知应用".into();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return "未知应用".into();
        }
        if let Some(path) = process_image_path(pid) {
            if let Some(stem) = path.file_stem() {
                return stem.to_string_lossy().to_string();
            }
        }
        let title = crate::winutil::window_title(hwnd);
        if title.is_empty() {
            "未知应用".into()
        } else {
            title
        }
    }
}

/// Capture the current source app, including a rendered icon when possible.
pub fn capture() -> SourceApp {
    let name = active_app_name();
    let mut app = SourceApp {
        name: name.clone(),
        icon: String::new(),
        platform: "win".into(),
        hash: String::new(),
    };
    // Try to grab the icon of the owning executable.
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if let Some(exe) = process_image_path(pid) {
            if let Some((png, hash)) = crate::icon::icon_for_exe(&exe) {
                app.icon = crate::util::encode_data_url("image/png", &png);
                app.hash = hash;
            }
        }
    }
    app
}

/// JSON string ready for `post.sourceApp`.
pub fn capture_json() -> String {
    capture().to_json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let app = SourceApp {
            name: "chrome".into(),
            icon: String::new(),
            platform: "win".into(),
            hash: String::new(),
        };
        let parsed = SourceApp::from_json(&app.to_json());
        assert_eq!(parsed.name, "chrome");
        assert_eq!(parsed.platform, "win");
    }

    #[test]
    fn missing_fields_default() {
        let parsed = SourceApp::from_json("{}");
        assert_eq!(parsed.name, "");
    }
}
