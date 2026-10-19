//! Clipboard integration.
//!
//! Replaces the original `clipboardListener.exe` (a .NET message-only window
//! listening for `WM_CLIPBOARDUPDATE`) and every `electron.clipboard` call.

use crate::model::{ContentType, Post};
use crate::util;
use crate::winutil::{self, MessageWindow};
use anyhow::Result;
use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, EnumClipboardFormats,
    GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

// Standard clipboard formats (Win32 SDK values).
pub const CF_TEXT: u32 = 1;
pub const CF_DIB: u32 = 8;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_HDROP: u32 = 15;
pub const CF_DIBV5: u32 = 17;

/// `WM_CLIPBOARDUPDATE`
pub const WM_CLIPBOARDUPDATE: u32 = 0x031D;

/// Registered format id for "HTML Format".
pub fn html_format() -> u32 {
    unsafe { RegisterClipboardFormatW(PCWSTR(winutil::to_wide("HTML Format").as_ptr())) }
}

/// Registered format id for "PNG".
pub fn png_format() -> u32 {
    unsafe { RegisterClipboardFormatW(PCWSTR(winutil::to_wide("PNG").as_ptr())) }
}

/// All formats currently on the clipboard.
pub fn available_formats() -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        if OpenClipboard(None).is_err() {
            return out;
        }
        let mut fmt = EnumClipboardFormats(0);
        while fmt != 0 {
            out.push(fmt);
            fmt = EnumClipboardFormats(fmt);
        }
        let _ = CloseClipboard();
    }
    out
}

fn has_format(fmt: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(fmt).is_ok() }
}

/// Read the raw bytes of a clipboard format.
fn read_raw(fmt: u32) -> Option<Vec<u8>> {
    unsafe {
        let h = GetClipboardData(fmt).ok()?;
        if h.is_invalid() {
            return None;
        }
        let hglobal = HGLOBAL(h.0);
        let size = GlobalSize(hglobal);
        if size == 0 {
            return None;
        }
        let ptr = GlobalLock(hglobal) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hglobal);
        Some(bytes)
    }
}

/// Read the clipboard as UTF-16 text (CF_UNICODETEXT).
fn read_unicode_text() -> Option<String> {
    unsafe {
        let h = GetClipboardData(CF_UNICODETEXT).ok()?;
        if h.is_invalid() {
            return None;
        }
        let hglobal = HGLOBAL(h.0);
        let ptr = GlobalLock(hglobal) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        let _ = GlobalUnlock(hglobal);
        Some(text)
    }
}

/// Extract the fragment portion of an "HTML Format" payload.
fn parse_html_format(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let field = |name: &str| -> Option<usize> {
        text.find(name).and_then(|i| {
            let rest = &text[i + name.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
    };
    let start = field("StartFragment:");
    let end = field("EndFragment:");
    if let (Some(s), Some(e)) = (start, end) {
        if s < e && e <= text.len() {
            return Some(text[s..e].to_string());
        }
    }
    let sh = field("StartHTML:");
    let eh = field("EndHTML:");
    if let (Some(s), Some(e)) = (sh, eh) {
        if s < e && e <= text.len() {
            return Some(text[s..e].to_string());
        }
    }
    if text.trim().is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Read a CF_HDROP file list.
fn read_file_list() -> Vec<PathBuf> {
    unsafe {
        let Ok(h) = GetClipboardData(CF_HDROP) else {
            return vec![];
        };
        if h.is_invalid() {
            return vec![];
        }
        let hdrop = windows::Win32::UI::Shell::HDROP(h.0);
        let count = windows::Win32::UI::Shell::DragQueryFileW(hdrop, 0xFFFFFFFF, None);
        let mut out = Vec::new();
        for i in 0..count {
            let len = windows::Win32::UI::Shell::DragQueryFileW(hdrop, i, None);
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u16; (len + 1) as usize];
            windows::Win32::UI::Shell::DragQueryFileW(hdrop, i, Some(&mut buf));
            let s: String = String::from_utf16_lossy(&buf[..len as usize]);
            if !s.is_empty() {
                out.push(PathBuf::from(s));
            }
        }
        out
    }
}

/// Convert a CF_DIB payload into PNG bytes using GDI.
fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBitmap, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, CBM_INIT, DIB_RGB_COLORS, HGDIOBJ,
    };
    if dib.len() < std::mem::size_of::<BITMAPINFOHEADER>() {
        return None;
    }
    unsafe {
        let header = &*(dib.as_ptr() as *const BITMAPINFOHEADER);
        let header_size = header.biSize as usize;
        if header_size == 0 || dib.len() < header_size {
            return None;
        }
        let width = header.biWidth;
        let height = header.biHeight.abs();
        if width <= 0 || height <= 0 {
            return None;
        }
        let bit_count = header.biBitCount as usize;
        let clr_used = if header.biClrUsed != 0 {
            header.biClrUsed as usize
        } else if bit_count <= 8 {
            1usize << bit_count
        } else {
            0
        };
        let palette_bytes = clr_used * 4;
        let data_offset = header_size + palette_bytes;
        if dib.len() < data_offset {
            return None;
        }
        let bits = dib.as_ptr().add(data_offset) as *const std::ffi::c_void;
        let pbmi = dib.as_ptr() as *const BITMAPINFO;

        let hdc = CreateCompatibleDC(None);
        if hdc.0.is_null() {
            return None;
        }
        let hbm = CreateDIBitmap(hdc, Some(header), CBM_INIT as u32, Some(bits), Some(pbmi), DIB_RGB_COLORS);
        if hbm.0.is_null() {
            let _ = DeleteDC(hdc);
            return None;
        }

        let mut out: BITMAPINFO = std::mem::zeroed();
        out.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        out.bmiHeader.biWidth = width;
        out.bmiHeader.biHeight = -height; // request a top-down buffer
        out.bmiHeader.biPlanes = 1;
        out.bmiHeader.biBitCount = 32;
        out.bmiHeader.biCompression = BI_RGB.0;

        let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
        let copied = GetDIBits(
            hdc,
            hbm,
            0,
            height as u32,
            Some(buf.as_mut_ptr() as *mut std::ffi::c_void),
            &mut out,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        let _ = DeleteDC(hdc);
        if copied == 0 {
            return None;
        }

        let mut rgba = Vec::with_capacity(buf.len());
        for px in buf.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        let img = image::RgbaImage::from_raw(width as u32, height as u32, rgba)?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut png, image::ImageFormat::Png)
            .ok()?;
        Some(png.into_inner())
    }
}

/// Read the bitmap on the clipboard and return PNG bytes.
pub fn read_image_png() -> Option<Vec<u8>> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        // A dedicated PNG format is cheaper to read when present.
        let png = if has_format(png_format()) {
            read_raw(png_format())
        } else {
            None
        };
        let dib = if png.is_none() {
            if has_format(CF_DIB) {
                read_raw(CF_DIB)
            } else if has_format(CF_DIBV5) {
                read_raw(CF_DIBV5)
            } else {
                None
            }
        } else {
            None
        };
        let _ = CloseClipboard();
        if let Some(p) = png {
            if p.len() > 8 {
                return Some(p);
            }
        }
        dib.and_then(|d| dib_to_png(&d))
    }
}

/// Everything the app needs out of the clipboard in one shot.
#[derive(Debug, Clone)]
pub enum Payload {
    Image(Vec<u8>),
    Files(Vec<PathBuf>),
    Text { text: String, html: Option<String> },
}

/// Snapshot of the clipboard, mirroring `getClipBoardContent()`.
pub fn read_payload() -> Option<Payload> {
    let formats = available_formats();
    if formats.is_empty() {
        return None;
    }
    let is_image = formats.contains(&CF_DIB) || formats.contains(&CF_DIBV5);
    let is_files = formats.contains(&CF_HDROP);
    let is_text = formats.contains(&CF_UNICODETEXT) || formats.contains(&CF_TEXT);

    if is_image {
        if let Some(png) = read_image_png() {
            if !png.is_empty() {
                return Some(Payload::Image(png));
            }
        }
    }

    if is_files {
        let files = read_file_list();
        if !files.is_empty() {
            return Some(Payload::Files(files));
        }
    }

    if is_text {
        unsafe {
            if OpenClipboard(None).is_err() {
                return None;
            }
            let html = read_raw(html_format()).and_then(|b| parse_html_format(&b));
            let text = read_unicode_text();
            let _ = CloseClipboard();
            if let Some(text) = text {
                if !text.trim().is_empty() {
                    return Some(Payload::Text { text, html });
                }
            }
        }
    }
    None
}

/// Build a `Post` payload from a clipboard snapshot (`createPost` + the
/// content-type dispatch of `getClipBoardContent`).
pub fn build_post(payload: &Payload, source_app_json: String) -> Option<Post> {
    let (content, content_type, title) = match payload {
        Payload::Image(png) => {
            let url = util::encode_data_url("image/png", png);
            (url, ContentType::Image, ContentType::Image.default_title())
        }
        Payload::Files(paths) => {
            let first = paths.first()?;
            let path = first.to_string_lossy().to_string();
            let size = winutil::path_size(first);
            let name = util::file_name(&path);
            // 0 = image file, 1 = regular file, 2 = folder
            let (kind, title) = if winutil::is_folder(&path) {
                (2, ContentType::Folder.default_title())
            } else if util::is_image_file(&path) {
                (0, "图片")
            } else {
                (1, ContentType::File.default_title())
            };
            (
                winutil::file_descriptor_json(&path, &name, size, kind),
                if kind == 2 { ContentType::Folder } else { ContentType::File },
                title,
            )
        }
        Payload::Text { text, html } => {
            if util::is_link(text) {
                (text.clone(), ContentType::Link, ContentType::Link.default_title())
            } else {
                let content = html
                    .clone()
                    .filter(|h| !h.trim().is_empty())
                    .unwrap_or_else(|| text.clone());
                (content, ContentType::Text, ContentType::Text.default_title())
            }
        }
    };

    let now = util::now_string();
    Some(Post {
        id: util::magic_id(),
        title: title.to_string(),
        content: content.replace('\u{0}', ""),
        description: Some("暂无".into()),
        content_type: content_type.as_i64(),
        source_app: Some(source_app_json),
        tags: Some("[]".into()),
        status: 0,
        group_id: None,
        group_name: Some(crate::model::ALL_GROUP_NAME.into()),
        edit_time: now,
        share_type: 0,
        is_delete: 0,
        ..Default::default()
    })
}

fn alloc_global(bytes: &[u8]) -> Option<HGLOBAL> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).ok()?;
        if h.is_invalid() {
            return None;
        }
        let ptr = GlobalLock(h) as *mut u8;
        if ptr.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(h);
        Some(h)
    }
}

/// Public clipboard write used by the renderer's "copy" actions.
pub fn write_text(plain: &str) -> bool {
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let mut wide: Vec<u16> = plain.encode_utf16().collect();
        wide.push(0);
        let bytes: Vec<u8> = wide.iter().flat_map(|u| u.to_le_bytes()).collect();
        if let Some(h) = alloc_global(&bytes) {
            let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0)));
        }
        let _ = CloseClipboard();
        true
    }
}

/// Write plain text plus an HTML alternative (`clipboard.write({text, html})`).
pub fn write_text_and_html(plain: &str, html: &str) -> bool {
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let payload = build_html_format(html);
        if let Some(h) = alloc_global(payload.as_bytes()) {
            let _ = SetClipboardData(html_format(), Some(HANDLE(h.0)));
        }
        let mut wide: Vec<u16> = plain.encode_utf16().collect();
        wide.push(0);
        let bytes: Vec<u8> = wide.iter().flat_map(|u| u.to_le_bytes()).collect();
        if let Some(h) = alloc_global(&bytes) {
            let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0)));
        }
        let _ = CloseClipboard();
        true
    }
}

/// Build a well-formed "HTML Format" clipboard payload.
pub fn build_html_format(fragment: &str) -> String {
    let prefix = "<html><body><!--StartFragment-->";
    let suffix = "<!--EndFragment--></body></html>";
    // Every offset is zero padded to 10 digits so the header length is fixed.
    let header_len = format!(
        "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\n",
        0, 0, 0, 0
    )
    .len();
    let start_html = header_len;
    let start_fragment = start_html + prefix.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + suffix.len();
    let header = format!(
        "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\n",
        start_html, end_html, start_fragment, end_fragment
    );
    format!("{header}{prefix}{fragment}{suffix}")
}

/// Put files on the clipboard as a drop list (`setClipboard.exe`).
pub fn write_files(paths: &[PathBuf]) -> bool {
    if paths.is_empty() {
        return false;
    }
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();

        // DROPFILES header: pFiles, pt, fNC, fWide
        let header_size: u32 = 20;
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&header_size.to_le_bytes());
        data.extend_from_slice(&0i32.to_le_bytes()); // pt.x
        data.extend_from_slice(&0i32.to_le_bytes()); // pt.y
        data.extend_from_slice(&0i32.to_le_bytes()); // fNC
        data.extend_from_slice(&1i32.to_le_bytes()); // fWide
        for p in paths {
            for u in p.to_string_lossy().encode_utf16() {
                data.extend_from_slice(&u.to_le_bytes());
            }
            data.extend_from_slice(&0u16.to_le_bytes());
        }
        data.extend_from_slice(&0u16.to_le_bytes()); // double NUL

        if let Some(h) = alloc_global(&data) {
            let _ = SetClipboardData(CF_HDROP, Some(HANDLE(h.0)));
        }
        let _ = CloseClipboard();
        true
    }
}

/// Watches the system clipboard, invoking `on_change` on every update.
pub struct ClipboardMonitor {
    _win: MessageWindow,
}

impl ClipboardMonitor {
    /// Start listening. `on_change` runs on the monitor's own thread.
    pub fn start(on_change: impl Fn() + Send + 'static) -> Option<ClipboardMonitor> {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let win = MessageWindow::new("clipassistant_clipboard_listener", move |msg, _w, _l| {
            if msg == WM_CLIPBOARDUPDATE {
                let _ = tx.send(());
            }
        })?;

        unsafe {
            let _ = AddClipboardFormatListener(win.hwnd);
        }

        let hwnd = win.hwnd;
        std::thread::spawn(move || {
            // A dedicated thread forwards change notifications so the callback
            // never runs inside the message pump itself.
            let _ = hwnd;
            for _ in rx {
                on_change();
            }
        });

        Some(ClipboardMonitor { _win: win })
    }
}

/// Add a clipboard format listener to an existing window (used by tests).
pub fn add_listener(hwnd: HWND) -> bool {
    unsafe { AddClipboardFormatListener(hwnd).is_ok() }
}

/// Number of formats currently available (cheap health check).
pub fn format_count() -> i32 {
    unsafe { windows::Win32::System::DataExchange::CountClipboardFormats() }
}

/// Convenience wrapper: write a single file/folder path.
pub fn write_file(path: &str) -> bool {
    write_files(&[PathBuf::from(path)])
}

/// Read only the plain text (used by tests).
pub fn read_text() -> Option<String> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let text = read_unicode_text();
        let _ = CloseClipboard();
        text
    }
}

/// Handle wrapper used by the paste module.
pub fn hwnd_is_valid(hwnd: HWND) -> bool {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::IsWindow;
        IsWindow(Some(hwnd)).as_bool()
    }
}

/// Result alias for internal helpers.
pub type ClipResult<T> = Result<T>;

/// Message loop helper re-export so callers do not need `winutil`.
pub fn run_message_loop() {
    MessageWindow::run_message_loop()
}

/// Unused parameter keeper.
pub fn _unused(_w: WPARAM, _l: LPARAM) {}
