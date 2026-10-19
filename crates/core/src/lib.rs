//! 剪贴助手 (ClipAssistant) — core library.
//!
//! Rust reimplementation of the Electron application's main-process behaviour:
//! clipboard capture, SQLite storage, settings persistence, global hotkeys,
//! paste automation, auto-start, tray and sound. Everything that does not need
//! a GPU/UI lives here so it can be unit tested without a display.

pub mod model;
pub mod util;

pub mod db;
pub mod settings;

#[cfg(windows)]
pub mod winutil;

#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
pub mod paste;
#[cfg(windows)]
pub mod sound;
#[cfg(windows)]
pub mod source_app;
#[cfg(windows)]
pub mod layout;
#[cfg(windows)]
pub mod autostart;

pub mod icon;
pub mod api;

pub use model::*;

/// Human readable application name.
pub const APP_NAME: &str = "剪贴助手";

/// Version reported in the UI.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Root of the per-user data directory (`~/.clipboard`).
///
/// `CLIPASSISTANT_DATA_DIR` overrides the location, which keeps tests and smoke runs
/// away from a real installation's history.
pub fn data_dir() -> std::path::PathBuf {
    if let Ok(custom) = std::env::var("CLIPASSISTANT_DATA_DIR") {
        if !custom.is_empty() {
            return std::path::PathBuf::from(custom);
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".clipboard")
}

/// Path of the SQLite database.
pub fn db_path() -> std::path::PathBuf {
    data_dir().join("db.sqlite")
}

/// Directory that holds the JSON settings store.
pub fn store_dir() -> std::path::PathBuf {
    data_dir().join("store")
}

/// Scratch directory for generated files (pasted images, cached icons).
pub fn tmp_dir() -> std::path::PathBuf {
    let dir = data_dir().join("tmp");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
