//! Settings persistence.
//!
//! The original app used `electron-store`, which is a single JSON file. We keep
//! the exact same file (`~/.clipboard/store/settings.json`) and the same three
//! top level keys (`settings`, `hotKeys`, `database_version`) so an existing
//! installation keeps working.

use crate::util::magic_id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const STORE_FILE: &str = "settings.json";
const KEY_SETTINGS: &str = "settings";
const KEY_HOTKEYS: &str = "hotKeys";
const KEY_DB_VERSION: &str = "database_version";

/// `clipboard.layout`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Horizontal,
    Vertical,
}

impl Default for Layout {
    fn default() -> Self {
        Layout::Horizontal
    }
}

/// `clipboard.theme`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::Light
    }
}

/// `clipboard.sounds.userCopy` / `userPaste`
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SoundInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sounds {
    #[serde(default = "yes")]
    pub open: bool,
    /// 0 default, 1 小黑子, 2 custom
    #[serde(rename = "type", default)]
    pub sound_type: i64,
    #[serde(default)]
    pub default_copy: String,
    #[serde(default)]
    pub default_paste: String,
    #[serde(default)]
    pub user_copy: SoundInfo,
    #[serde(default)]
    pub user_paste: SoundInfo,
}

impl Default for Sounds {
    fn default() -> Self {
        Self {
            open: true,
            sound_type: 0,
            default_copy: String::new(),
            default_paste: String::new(),
            user_copy: SoundInfo::default(),
            user_paste: SoundInfo::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardSettings {
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub sounds: Sounds,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            layout: Layout::Horizontal,
            sounds: Sounds::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutKeys {
    #[serde(default = "yes")]
    pub quick_paste_enable: bool,
    #[serde(default = "default_show_or_hide")]
    pub show_or_hide: String,
    #[serde(default = "default_prefix")]
    pub quick_paste: String,
    #[serde(default = "default_previous_group")]
    pub previous_group: String,
    #[serde(default = "default_next_group")]
    pub next_group: String,
}

fn yes() -> bool {
    true
}

#[cfg(target_os = "macos")]
fn default_prefix() -> String {
    "command".into()
}
#[cfg(not(target_os = "macos"))]
fn default_prefix() -> String {
    "ctrl".into()
}

fn default_show_or_hide() -> String {
    format!("{}+`", default_prefix())
}
fn default_previous_group() -> String {
    format!("{}+[", default_prefix())
}
fn default_next_group() -> String {
    format!("{}+]", default_prefix())
}

impl Default for ShortcutKeys {
    fn default() -> Self {
        Self {
            quick_paste_enable: true,
            show_or_hide: default_show_or_hide(),
            quick_paste: default_prefix(),
            previous_group: default_previous_group(),
            next_group: default_next_group(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cloud {
    #[serde(default)]
    pub auto: bool,
}

/// `general.historyCache` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryCache {
    Second,
    Day,
    Week,
    Month,
    Quarter,
    Half,
    Year,
    Infinity,
}

impl Default for HistoryCache {
    fn default() -> Self {
        HistoryCache::Infinity
    }
}

impl HistoryCache {
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryCache::Second => "second",
            HistoryCache::Day => "day",
            HistoryCache::Week => "week",
            HistoryCache::Month => "month",
            HistoryCache::Quarter => "quarter",
            HistoryCache::Half => "half",
            HistoryCache::Year => "year",
            HistoryCache::Infinity => "infinity",
        }
    }

    /// Slider position used by the general settings page (0..120 step 20).
    pub fn slider_value(self) -> i64 {
        match self {
            HistoryCache::Day => 0,
            HistoryCache::Week => 20,
            HistoryCache::Month => 40,
            HistoryCache::Quarter => 60,
            HistoryCache::Half => 80,
            HistoryCache::Year => 100,
            HistoryCache::Infinity => 120,
            HistoryCache::Second => 0,
        }
    }

    pub fn from_slider(v: i64) -> Self {
        match v {
            0 => HistoryCache::Day,
            20 => HistoryCache::Week,
            40 => HistoryCache::Month,
            60 => HistoryCache::Quarter,
            80 => HistoryCache::Half,
            100 => HistoryCache::Year,
            _ => HistoryCache::Infinity,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct General {
    #[serde(default = "yes")]
    pub auto_open: bool,
    #[serde(default)]
    pub min_tray: bool,
    #[serde(default = "yes")]
    pub menu_icon: bool,
    #[serde(default)]
    pub history_cache: HistoryCache,
}

impl Default for General {
    fn default() -> Self {
        Self {
            auto_open: true,
            min_tray: false,
            menu_icon: true,
            history_cache: HistoryCache::Infinity,
        }
    }
}

/// The complete `settings` object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub clipboard: ClipboardSettings,
    #[serde(default)]
    pub shortcut_keys: ShortcutKeys,
    #[serde(default)]
    pub cloud: Cloud,
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clipboard: ClipboardSettings::default(),
            shortcut_keys: ShortcutKeys::default(),
            cloud: Cloud::default(),
            general: General::default(),
            width: 0,
            height: 0,
        }
    }
}

/// A per-post hotkey (`hotKeys` map in the original store).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppointShortcut {
    pub id: i64,
    pub content_type: i64,
    pub shortcut: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// Thread-safe view over the JSON store file.
pub struct Store {
    path: PathBuf,
    data: Mutex<serde_json::Value>,
}

impl Store {
    /// Open (or create) the store at the default location.
    pub fn open_default() -> anyhow::Result<Arc<Store>> {
        let dir = crate::store_dir();
        std::fs::create_dir_all(&dir)?;
        Store::open(dir.join(STORE_FILE))
    }

    pub fn open(path: PathBuf) -> anyhow::Result<Arc<Store>> {
        let data = match std::fs::read_to_string(&path) {
            Ok(text) if !text.trim().is_empty() => {
                serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}))
            }
            _ => serde_json::json!({}),
        };
        Ok(Arc::new(Store {
            path,
            data: Mutex::new(data),
        }))
    }

    fn flush(&self, data: &serde_json::Value) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(data)?;
        // Write atomically so a crash cannot leave a half-written store.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Current settings, merged over the defaults.
    pub fn settings(&self) -> Settings {
        let guard = self.data.lock().unwrap();
        let raw = guard.get(KEY_SETTINGS).cloned().unwrap_or(serde_json::json!({}));
        // Merge defaults first (serde defaults already handle missing fields,
        // but nested objects need the same treatment).
        let mut base = serde_json::to_value(Settings::default()).unwrap_or_default();
        merge(&mut base, &raw);
        serde_json::from_value(base).unwrap_or_default()
    }

    /// Mutate and persist the settings object.
    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        let mut s = self.settings();
        f(&mut s);
        let value = serde_json::to_value(&s).unwrap_or_default();
        let snapshot = {
            let mut guard = self.data.lock().unwrap();
            guard[KEY_SETTINGS] = value;
            guard.clone()
        };
        let _ = self.flush(&snapshot);
    }

    pub fn hot_keys(&self) -> BTreeMap<String, AppointShortcut> {
        let guard = self.data.lock().unwrap();
        guard
            .get(KEY_HOTKEYS)
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    pub fn set_hot_key(&self, key: &str, value: AppointShortcut) {
        let mut map = self.hot_keys();
        map.insert(key.to_string(), value);
        self.write_hot_keys(&map);
    }

    pub fn remove_hot_key(&self, key: &str) -> Option<AppointShortcut> {
        let mut map = self.hot_keys();
        let removed = map.remove(key);
        self.write_hot_keys(&map);
        removed
    }

    pub fn replace_hot_keys(&self, map: BTreeMap<String, AppointShortcut>) {
        self.write_hot_keys(&map);
    }

    fn write_hot_keys(&self, map: &BTreeMap<String, AppointShortcut>) {
        let value = serde_json::to_value(map).unwrap_or(serde_json::json!({}));
        let snapshot = {
            let mut guard = self.data.lock().unwrap();
            guard[KEY_HOTKEYS] = value;
            guard.clone()
        };
        let _ = self.flush(&snapshot);
    }

    pub fn db_version(&self) -> Option<String> {
        let guard = self.data.lock().unwrap();
        guard.get(KEY_DB_VERSION).and_then(|v| v.as_str().map(String::from))
    }

    pub fn set_db_version(&self, version: &str) {
        let snapshot = {
            let mut guard = self.data.lock().unwrap();
            guard[KEY_DB_VERSION] = serde_json::Value::String(version.to_string());
            guard.clone()
        };
        let _ = self.flush(&snapshot);
    }
}

/// Recursively overlay `patch` on top of `base` (objects only).
fn merge(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k.clone()).or_insert(serde_json::Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// A brand new group payload (original `createGroup()`).
pub fn new_group_payload() -> crate::model::PostGroup {
    let n = rand::random::<u32>() % 10_000;
    crate::model::PostGroup {
        id: magic_id(),
        group_name: format!("未命名{n}"),
        is_delete: 0,
        status: 0,
        sorted: 99_999,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store() -> (tempfile::TempDir, Arc<Store>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("settings.json")).unwrap();
        (dir, store)
    }

    #[test]
    fn defaults_are_sane() {
        let (_d, s) = tmp_store();
        let cfg = s.settings();
        assert!(cfg.general.auto_open);
        assert_eq!(cfg.clipboard.layout, Layout::Horizontal);
        assert!(cfg.shortcut_keys.show_or_hide.ends_with('`'));
    }

    #[test]
    fn settings_roundtrip() {
        let (_d, s) = tmp_store();
        s.update_settings(|c| {
            c.clipboard.layout = Layout::Vertical;
            c.clipboard.theme = Theme::Dark;
            c.general.history_cache = HistoryCache::Week;
        });
        let again = Store::open(PathBuf::from(&s.path)).unwrap();
        let cfg = again.settings();
        assert_eq!(cfg.clipboard.layout, Layout::Vertical);
        assert_eq!(cfg.clipboard.theme, Theme::Dark);
        assert_eq!(cfg.general.history_cache, HistoryCache::Week);
    }

    #[test]
    fn hotkeys_roundtrip() {
        let (_d, s) = tmp_store();
        s.set_hot_key(
            "123",
            AppointShortcut {
                id: 123,
                content_type: 0,
                shortcut: "ctrl+1".into(),
                name: None,
            },
        );
        assert_eq!(s.hot_keys().len(), 1);
        assert!(s.remove_hot_key("123").is_some());
        assert!(s.hot_keys().is_empty());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(
            &p,
            r#"{"settings":{"clipboard":{"layout":"vertical","future":1},"general":{"historyCache":"year"}},"other":true}"#,
        )
        .unwrap();
        let s = Store::open(p).unwrap();
        let cfg = s.settings();
        assert_eq!(cfg.clipboard.layout, Layout::Vertical);
        assert_eq!(cfg.general.history_cache, HistoryCache::Year);
        assert!(cfg.general.auto_open);
    }
}
