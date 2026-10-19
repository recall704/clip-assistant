//! Paste automation.
//!
//! Reproduces the original flow: put the payload on the clipboard, bring the
//! previously focused window back to the front, then synthesise `Ctrl+V`
//! (`robotjs.keyTap` + `setHandle.exe` in the original).

use crate::clipboard;
use crate::model::{ContentType, FileDescriptor, Post};
use crate::util;
use crate::winutil;
use std::path::PathBuf;
use std::sync::Mutex;
use windows::Win32::Foundation::HWND;

/// Foreground window captured just before the panel appeared.
static SAVED_HANDLE: Mutex<isize> = Mutex::new(0);

pub fn save_foreground_handle() {
    let hwnd = winutil::foreground_window();
    *SAVED_HANDLE.lock().unwrap() = hwnd.0 as isize;
}

pub fn saved_handle() -> HWND {
    HWND(*SAVED_HANDLE.lock().unwrap() as *mut _)
}

/// Files written for image pastes are removed again after this delay.
const IMAGE_FILE_TTL_MS: u64 = 10_000;

/// What a post contributes to the clipboard.
pub enum Printable {
    Text { plain: String, html: Option<String> },
    Files(Vec<PathBuf>),
    Nothing,
}

/// `getPasteContent()` — derive the text/html pair for a post.
pub fn render_text(post: &Post, as_plain: bool) -> (String, Option<String>) {
    match post.content_type() {
        ContentType::Link => (post.content.clone(), None),
        ContentType::Text => {
            if as_plain {
                (util::html_to_text(&post.content), None)
            } else {
                (util::html_to_text(&post.content), Some(post.content.clone()))
            }
        }
        _ => (String::new(), None),
    }
}

/// Decide what to place on the clipboard for `post`.
pub fn printable_for(post: &Post, as_plain: bool) -> Printable {
    match post.content_type() {
        ContentType::Text | ContentType::Link => {
            let (plain, html) = render_text(post, as_plain);
            Printable::Text {
                plain,
                html: if as_plain { None } else { html },
            }
        }
        ContentType::Image => match util::decode_data_url(&post.content) {
            Some((_mime, bytes)) => match write_temp_image(&bytes) {
                Some(path) => Printable::Files(vec![path]),
                None => Printable::Nothing,
            },
            None => Printable::Nothing,
        },
        ContentType::File | ContentType::Folder => {
            match serde_json::from_str::<FileDescriptor>(&post.content) {
                Ok(fd) => Printable::Files(vec![PathBuf::from(fd.content)]),
                Err(_) => Printable::Nothing,
            }
        }
    }
}

/// Write PNG bytes to a scratch file so the shell can paste it as a file.
pub fn write_temp_image(bytes: &[u8]) -> Option<PathBuf> {
    let dir = crate::tmp_dir();
    let name = format!("file_{}_{}.png", chrono::Utc::now().timestamp_millis(), rand::random::<u16>());
    let path = dir.join(name);
    std::fs::write(&path, bytes).ok()?;
    let cleanup = path.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(IMAGE_FILE_TTL_MS));
        let _ = std::fs::remove_file(cleanup);
    });
    Some(path)
}

/// Put `printable` on the clipboard, restore the target window and paste.
pub fn paste(printable: Printable, target: Option<HWND>, send_keys: bool) {
    match printable {
        Printable::Text { plain, html } => match html {
            Some(h) if !h.trim().is_empty() => {
                clipboard::write_text_and_html(&plain, &h);
            }
            _ => {
                clipboard::write_text(&plain);
            }
        },
        Printable::Files(paths) => {
            clipboard::write_files(&paths);
        }
        Printable::Nothing => return,
    }
    focus_and_paste(target, send_keys);
}

/// Convenience: paste a whole post.
pub fn paste_post(post: &Post, as_plain: bool, target: Option<HWND>, send_keys: bool) {
    let printable = printable_for(post, as_plain);
    paste(printable, target, send_keys);
}

/// Restore focus (if a target is known) and synthesise the paste keystroke.
pub fn focus_and_paste(target: Option<HWND>, send_keys: bool) {
    let hwnd = target.unwrap_or_else(saved_handle);
    let raw = hwnd.0 as isize;
    if !send_keys {
        // Called when the UI is closing/animating: still restore focus so the
        // user lands back in the app they came from.
        if raw != 0 {
            winutil::set_foreground_window(HWND(raw as *mut _));
        }
        return;
    }
    std::thread::spawn(move || {
        if raw != 0 {
            winutil::set_foreground_window(HWND(raw as *mut _));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        winutil::send_ctrl_v();
    });
}

/// Copy a post's payload onto the clipboard without pasting.
pub fn copy_post(post: &Post, as_plain: bool) {
    match printable_for(post, as_plain) {
        Printable::Text { plain, html } => match html {
            Some(h) if !h.trim().is_empty() => {
                clipboard::write_text_and_html(&plain, &h);
            }
            _ => {
                clipboard::write_text(&plain);
            }
        },
        Printable::Files(paths) => {
            if paths.len() == 1 {
                clipboard::write_file(&paths[0].to_string_lossy());
            } else {
                clipboard::write_files(&paths);
            }
        }
        Printable::Nothing => {}
    }
}

/// Build the file descriptor JSON used on a file/folder card.
pub fn file_descriptor(content: &str, title: &str, size: i64, kind: i64) -> String {
    winutil::file_descriptor_json(content, title, size, kind)
}
