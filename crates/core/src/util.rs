//! Small helpers shared across the app, ported from the original `utils`.

use base64::Engine;
use chrono::{DateTime, Local, TimeZone};
use rand::Rng;
use sha2::{Digest, Sha256};

/// 15 digit numeric identifier, matching the original `magicId()`.
pub fn magic_id() -> i64 {
    let mut rng = rand::thread_rng();
    let mut s = String::with_capacity(15);
    // First digit 1-9 so the value is always 15 digits long.
    s.push(char::from_digit(rng.gen_range(1..10), 10).unwrap());
    for _ in 0..14 {
        s.push(char::from_digit(rng.gen_range(0..10), 10).unwrap());
    }
    s.parse().unwrap_or(0)
}

/// Format a timestamp the way the original `formatDateToString` does.
pub fn format_date_to_string(ts: i64) -> String {
    let dt = Local.timestamp_millis_opt(ts).single().unwrap_or_else(Local::now);
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn now_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Parse the `YYYY-MM-DD HH:mm:ss` strings stored in SQLite (local time).
pub fn parse_local(s: &str) -> Option<DateTime<Local>> {
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|n| Local.from_local_datetime(&n).single())
}

/// "刚刚 / 3 分钟前 / ..." label used on post cards.
pub fn time_ago(s: &str) -> String {
    let Some(dt) = parse_local(s) else {
        return s.to_string();
    };
    let secs = (Local::now() - dt).num_seconds().max(0);
    let (n, unit) = if secs < 60 {
        (secs, "秒")
    } else if secs < 3600 {
        (secs / 60, "分钟")
    } else if secs < 86_400 {
        (secs / 3600, "小时")
    } else if secs < 2_592_000 {
        (secs / 86_400, "天")
    } else if secs < 31_104_000 {
        (secs / 2_592_000, "个月")
    } else {
        (secs / 31_104_000, "年")
    };
    if n <= 1 {
        "刚刚".into()
    } else {
        format!("{n} 个{unit}前")
    }
}

/// `isLink()` from the original: http/https/ftp URL detection.
pub fn is_link(text: &str) -> bool {
    let t = text.trim();
    for scheme in ["http://", "https://", "ftp://"] {
        if let Some(rest) = t.strip_prefix(scheme) {
            if rest.is_empty() {
                return false;
            }
            // No whitespace and not consisting solely of punctuation.
            if rest.chars().any(char::is_whitespace) {
                return false;
            }
            let first = rest.chars().next().unwrap();
            if matches!(first, '/' | '$' | '.' | '?' | '#') {
                return false;
            }
            return true;
        }
    }
    false
}

/// File or folder name for a path (portable separators).
pub fn file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    trimmed
        .rsplit(['\\', '/'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("未知")
        .to_string()
}

/// Human readable byte count, `utils/index.ts → formatSize`.
pub fn format_size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.2} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} MB", b / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", b / 1024.0 / 1024.0 / 1024.0)
    }
}

/// SHA-256 of a byte payload as a lower-case hex string.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

/// Decode a `data:<mime>;base64,<payload>` URL.
pub fn decode_data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mime = meta.split(';').next().unwrap_or("").to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .ok()?;
    Some((mime, bytes))
}

/// Build a `data:<mime>;base64,<payload>` URL.
pub fn encode_data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Convert stored HTML into plain text, mirroring `html-to-text` usage.
pub fn html_to_text(html: &str) -> String {
    if html.is_empty() {
        return String::new();
    }
    html2text::from_read(html.as_bytes(), 1_000_000).unwrap_or_default()
}

/// Escapes `<`/`>` (used when a "HTML" payload has no real tags).
pub fn escape_angles(s: &str) -> String {
    s.replace('<', "&lt;").replace('>', "&gt;")
}

pub fn unescape_angles(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">")
}

/// Whether a payload should be treated as HTML rather than plain text.
pub fn looks_like_html(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.contains("<meta") || lower.contains("<div") || lower.contains("<p>") || lower.contains("<span")
}

/// Extension (lower-case, with dot) of a path.
pub fn extension(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_ascii_lowercase()))
        .unwrap_or_default()
}

const IMAGE_EXT: [&str; 6] = [".jpg", ".jpeg", ".png", ".gif", ".bmp", ".svg"];

pub fn is_image_file(path: &str) -> bool {
    IMAGE_EXT.contains(&extension(path).as_str())
}

/// Source-app icon path on the local disk (cached downloads).
pub fn icon_cache_dir() -> std::path::PathBuf {
    crate::data_dir().join("icons")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_id_is_15_digits() {
        for _ in 0..100 {
            let id = magic_id();
            assert!(id >= 100_000_000_000_000, "{id}");
            assert!(id <= 999_999_999_999_999, "{id}");
        }
    }

    #[test]
    fn links() {
        assert!(is_link("https://example.com/a?b=1"));
        assert!(is_link("http://a.cn"));
        assert!(!is_link("hello world"));
        assert!(!is_link("https://"));
        assert!(!is_link("ftp://"));
    }

    #[test]
    fn names_and_sizes() {
        assert_eq!(file_name("C:\\Users\\me\\a.txt"), "a.txt");
        assert_eq!(file_name("/Users/me/Documents/"), "Documents");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.00 KB");
    }

    #[test]
    fn data_url_roundtrip() {
        let url = encode_data_url("image/png", b"hello");
        let (mime, bytes) = decode_data_url(&url).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, b"hello");
    }
}
