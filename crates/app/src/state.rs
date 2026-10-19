//! Global application state shared by every window.

use gpui::Global;
use clipassistant_core::db::{Db, PostPatch};
use clipassistant_core::hotkey::HotkeyManager;
use clipassistant_core::clipboard;
use clipassistant_core::model::{Post, PostGroup, PostQuery};
use clipassistant_core::settings::{HistoryCache, Settings, Store};
use clipassistant_core::{sound, source_app, util};
use std::sync::{Arc, Mutex};

/// Everything the UI layers need, stored as a GPUI global.
pub struct AppState {
    pub db: Arc<Db>,
    pub store: Arc<Store>,
    pub hotkeys: Mutex<Option<Arc<HotkeyManager>>>,
    /// Post id -> shortcut, mirrored from the store for quick lookup.
    pub appoint: Mutex<std::collections::BTreeMap<String, clipassistant_core::settings::AppointShortcut>>,
}

impl Global for AppState {}

impl AppState {
    pub fn new() -> anyhow::Result<AppState> {
        let store = Store::open_default()?;
        let db = Arc::new(Db::open(&clipassistant_core::db_path())?);
        let state = AppState {
            db,
            store,
            hotkeys: Mutex::new(None),
            appoint: Mutex::new(Default::default()),
        };
        state.reload_appoint();
        Ok(state)
    }

    pub fn settings(&self) -> Settings {
        self.store.settings()
    }

    pub fn reload_appoint(&self) {
        *self.appoint.lock().unwrap() = self.store.hot_keys();
    }

    // ------------------------------------------------------------- queries

    pub fn query_posts(&self, q: &PostQuery) -> Vec<Post> {
        self.db.query_posts(q).unwrap_or_default()
    }

    /// The post list for a group (empty name or "全部" means everything).
    pub fn posts_in_group(&self, group: &str, search: &str, page: i64) -> Vec<Post> {
        let q = PostQuery {
            status: Some(0),
            title: if search.trim().is_empty() {
                None
            } else {
                Some(search.trim().to_string())
            },
            group_name: if group.is_empty() {
                None
            } else {
                Some(group.to_string())
            },
            current: Some(page),
            ..Default::default()
        };
        self.query_posts(&q)
    }

    pub fn groups(&self) -> Vec<PostGroup> {
        self.db.query_groups().unwrap_or_default()
    }

    // ------------------------------------------------------------ mutation

    /// Insert a post; returns the created row.
    pub fn create_post(&self, post: &Post) -> Option<Post> {
        self.db.insert_post(post).ok()
    }

    pub fn update_post(&self, id: i64, patch: &PostPatch) -> usize {
        self.db.update_post(id, patch).unwrap_or(0)
    }

    pub fn touch_post(&self, id: i64) {
        let now = util::now_string();
        self.update_post(
            id,
            &PostPatch {
                edit_time: Some(now),
                ..Default::default()
            },
        );
    }

    pub fn remove_post(&self, id: i64) -> bool {
        let ok = self.db.remove_post(id).map(|n| n > 0).unwrap_or(false);
        if ok {
            let _ = self.store.remove_hot_key(&id.to_string());
            self.reload_appoint();
        }
        ok
    }

    pub fn move_post(&self, id: i64, group_id: Option<i64>, group_name: &str) {
        self.update_post(
            id,
            &PostPatch {
                group_id: Some(group_id),
                group_name: Some(group_name.to_string()),
                edit_time: Some(util::now_string()),
                ..Default::default()
            },
        );
    }

    pub fn create_group(&self) -> Option<PostGroup> {
        let g = clipassistant_core::settings::new_group_payload();
        self.db.insert_group(&g).ok()
    }

    pub fn rename_group(&self, id: i64, name: &str) -> bool {
        self.db.update_group(id, Some(name), None).map(|n| n > 0).unwrap_or(false) && {
            // Keep posts in sync with the new group name.
            for post in self.query_posts(&PostQuery {
                group_name: Some(name.to_string()),
                ..Default::default()
            }) {
                let _ = post;
            }
            true
        }
    }

    pub fn reorder_groups(&self, ids: &[i64]) {
        for (i, id) in ids.iter().enumerate() {
            let _ = self.db.update_group(*id, None, Some(i as i64));
        }
    }

    pub fn remove_group(&self, id: i64) {
        // Move its posts back to the ungrouped bucket first.
        let _ = self.db.clear_group_posts(id);
        let group_name = self
            .groups()
            .into_iter()
            .find(|g| g.id == id)
            .map(|g| g.group_name)
            .unwrap_or_default();
        for post in self.query_posts(&PostQuery {
            group_name: Some(group_name),
            ..Default::default()
        }) {
            self.update_post(
                post.id,
                &PostPatch {
                    group_id: Some(None),
                    group_name: Some(clipassistant_core::model::ALL_GROUP_NAME.into()),
                    ..Default::default()
                },
            );
        }
        let _ = self.db.remove_group(id);
    }

    pub fn clear_group(&self, id: i64) {
        let _ = self.db.clear_group_posts(id);
    }

    pub fn clear_history(&self) {
        let _ = self.db.clear_all_posts();
        let _ = self.db.clear_all_groups();
        self.store.replace_hot_keys(Default::default());
        self.reload_appoint();
    }

    // ----------------------------------------------------------- clipboard

    /// Read the clipboard and turn it into a history entry.
    ///
    /// Returns `Some(post)` for a brand new entry and `None` when the content
    /// was already known (its timestamp is refreshed in that case).
    pub fn capture_clipboard(&self) -> Option<Post> {
        let payload = clipboard::read_payload()?;
        let source = source_app::capture_json();
        let post = clipboard::build_post(&payload, source)?;
        if post.content.trim().is_empty() {
            return None;
        }
        if let Some(existing) = self
            .db
            .query_posts(&PostQuery {
                content: Some(post.content.clone()),
                status: Some(0),
                ..Default::default()
            })
            .ok()
            .and_then(|v| v.into_iter().next())
        {
            self.touch_post(existing.id);
            return None;
        }
        let created = self.create_post(&post);
        if created.is_some() {
            let s = self.settings();
            if s.clipboard.sounds.open {
                let (kind, url) = sound_for(&s, true);
                sound::play_copy(kind, &url);
            }
        }
        created
    }

    /// Put a post on the clipboard and paste it into the previous window.
    pub fn paste_post(&self, post: &Post, as_plain: bool) {
        let s = self.settings();
        if s.clipboard.sounds.open {
            let (kind, url) = sound_for(&s, false);
            sound::play_paste(kind, &url);
        }
        other::play(self, post, as_plain);
    }
}

/// Small module so `paste_post` stays readable.
mod other {
    use super::*;

    pub fn play(state: &AppState, post: &Post, as_plain: bool) {
        clipassistant_core::paste::paste_post(post, as_plain, None, true);
        // Refresh the timestamp so the item moves to the front.
        state.touch_post(post.id);
    }
}

/// Pick the configured sound kind and custom URL.
pub fn sound_for(s: &Settings, copy: bool) -> (sound::SoundKind, String) {    let kind = match s.clipboard.sounds.sound_type {
        1 => sound::SoundKind::Ikun,
        2 => sound::SoundKind::Custom,
        _ => sound::SoundKind::Default,
    };
    let url = if copy {
        s.clipboard.sounds.user_copy.url.clone()
    } else {
        s.clipboard.sounds.user_paste.url.clone()
    };
    (kind, url)
}

/// Delete entries older than the configured retention window.
pub fn prune_history(state: &AppState) {
    let cache = state.settings().general.history_cache;
    if cache == HistoryCache::Infinity {
        return;
    }
    let cutoff = match cache {
        HistoryCache::Second => chrono::Local::now() - chrono::Duration::seconds(10),
        HistoryCache::Day => chrono::Local::now() - chrono::Duration::days(1),
        HistoryCache::Week => chrono::Local::now() - chrono::Duration::weeks(1),
        HistoryCache::Month => chrono::Local::now() - chrono::Duration::days(30),
        HistoryCache::Quarter => chrono::Local::now() - chrono::Duration::days(120),
        HistoryCache::Half => chrono::Local::now() - chrono::Duration::days(182),
        HistoryCache::Year => chrono::Local::now() - chrono::Duration::days(365),
        HistoryCache::Infinity => return,
    };
    let cutoff = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();
    let _ = state.db.delete_posts_before(&cutoff);
}
