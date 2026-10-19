//! Source-application icons.
//!
//! The original uploaded an app icon to its cloud and cached the resulting URL
//! in the `post_icon` table. We extract the icon locally from the executable
//! and cache the PNG on disk, keyed by its SHA-256, which removes the network
//! dependency while keeping the same table for compatibility.

use crate::util;
use std::path::{Path, PathBuf};

/// Extract the icon of an executable as PNG bytes plus its content hash.
pub fn icon_for_exe(exe: &Path) -> Option<(Vec<u8>, String)> {
    let png = extract_icon_png(exe, 64)?;
    let hash = util::sha256_hex(&png);
    let path = cache_path(&hash);
    if !path.exists() {
        let _ = std::fs::write(&path, &png);
    }
    Some((png, hash))
}

fn cache_dir() -> PathBuf {
    let dir = crate::data_dir().join("icons");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn cache_path(hash: &str) -> PathBuf {
    cache_dir().join(format!("{hash}.png"))
}

/// Load a previously cached icon.
pub fn load_icon(hash: &str) -> Option<Vec<u8>> {
    std::fs::read(cache_path(hash)).ok()
}

/// Store an icon payload under its hash.
pub fn store_icon(hash: &str, bytes: &[u8]) -> Option<PathBuf> {
    let path = cache_path(hash);
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Render an executable's large icon into a PNG buffer.
pub fn extract_icon_png(exe: &Path, size: i32) -> Option<Vec<u8>> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL};

    let wide: Vec<u16> = exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut large = windows::Win32::UI::WindowsAndMessaging::HICON(std::ptr::null_mut());
        let count = ExtractIconExW(PCWSTR(wide.as_ptr()), 0, Some(&mut large), None, 1);
        if count == 0 || large.0.is_null() {
            return None;
        }

        let hdc = CreateCompatibleDC(None);
        if hdc.0.is_null() {
            let _ = DestroyIcon(large);
            return None;
        }

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = size;
        bmi.bmiHeader.biHeight = -size; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbm = CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        let Ok(hbm) = hbm else {
            let _ = DeleteDC(hdc);
            let _ = DestroyIcon(large);
            return None;
        };
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(hbm.0));
            let _ = DeleteDC(hdc);
            let _ = DestroyIcon(large);
            return None;
        }

        let old = SelectObject(hdc, HGDIOBJ(hbm.0));
        let drew = DrawIconEx(hdc, 0, 0, large, size, size, 0, None, DI_NORMAL).is_ok();
        let _ = SelectObject(hdc, old);
        let _ = DestroyIcon(large);
        let _ = DeleteDC(hdc);

        let out = if drew {
            let len = (size * size * 4) as usize;
            let slice = std::slice::from_raw_parts(bits as *const u8, len);
            let mut rgba = Vec::with_capacity(len);
            let mut any_alpha = false;
            for px in slice.chunks_exact(4) {
                let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
                if a != 0 {
                    any_alpha = true;
                }
                rgba.extend_from_slice(&[r, g, b, a]);
            }
            // Some legacy icons have no alpha channel at all.
            if !any_alpha {
                for px in rgba.chunks_exact_mut(4) {
                    px[3] = 255;
                }
            }
            let img = image::RgbaImage::from_raw(size as u32, size as u32, rgba)?;
            let mut buf = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut buf, image::ImageFormat::Png)
                .ok()?;
            Some(buf.into_inner())
        } else {
            None
        };
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_is_stable() {
        let a = cache_path("abc");
        let b = cache_path("abc");
        assert_eq!(a, b);
        assert!(a.to_string_lossy().ends_with("abc.png"));
    }

    #[test]
    fn store_and_load_icon() {
        let hash = format!("test-{}", util::magic_id());
        store_icon(&hash, b"png-bytes").unwrap();
        assert_eq!(load_icon(&hash).unwrap(), b"png-bytes");
    }
}
