//! Applying settings changes: hotkeys, tray, auto-start, theme and layout.

use crate::state::{prune_history, AppState};
use crate::windows;
use gpui::App;
use clipassistant_core::hotkey::ids;
use clipassistant_core::model::PostQuery;
use clipassistant_core::settings::{HistoryCache, Layout, Theme};
use clipassistant_core::sound;

/// Repaint every open window (theme / layout switches).
pub fn notify_all(cx: &mut App) {
    cx.refresh_windows();
}

pub fn set_theme(cx: &mut App, theme: Theme) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.clipboard.theme = theme);
    notify_all(cx);
}

pub fn set_layout(cx: &mut App, layout: Layout) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.clipboard.layout = layout);
    windows::redock_panel(cx);
    notify_all(cx);
}

pub fn set_sound_open(cx: &mut App, open: bool) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.clipboard.sounds.open = open);
}

pub fn set_sound_type(cx: &mut App, kind: i64) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.clipboard.sounds.sound_type = kind);
}

pub fn set_user_sound(cx: &mut App, copy: bool, info: clipassistant_core::settings::SoundInfo) {
    cx.global::<AppState>().store.update_settings(|s| {
        if copy {
            s.clipboard.sounds.user_copy = info;
        } else {
            s.clipboard.sounds.user_paste = info;
        }
    });
}

pub fn set_auto_run(cx: &mut App, open: bool) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.general.auto_open = open);
    clipassistant_core::autostart::set_enabled(open);
}

pub fn set_min_tray(cx: &mut App, open: bool) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.general.min_tray = open);
}

pub fn set_menu_icon(cx: &mut App, open: bool) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.general.menu_icon = open);
    crate::tray::set_visible(open);
}

pub fn set_history_cache(cx: &mut App, cache: HistoryCache) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.general.history_cache = cache);
    prune_history(cx.global::<AppState>());
}

macro_rules! set_shortcut {
    ($name:ident, $field:ident) => {
        pub fn $name(cx: &mut App, key: &str) {
            cx.global::<AppState>()
                .store
                .update_settings(|s| s.shortcut_keys.$field = key.to_string());
            register_hotkeys(cx);
        }
    };
}

set_shortcut!(set_show_or_hide, show_or_hide);
set_shortcut!(set_previous_group, previous_group);
set_shortcut!(set_next_group, next_group);
set_shortcut!(set_quick_paste, quick_paste);

pub fn set_quick_paste_enabled(cx: &mut App, enabled: bool) {
    cx.global::<AppState>()
        .store
        .update_settings(|s| s.shortcut_keys.quick_paste_enable = enabled);
    register_hotkeys(cx);
}

/// (Re)register every global shortcut from the current settings.
pub fn register_hotkeys(cx: &mut App) {
    let (settings, appoint, manager) = {
        let state = cx.global::<AppState>();
        (
            state.settings(),
            state.appoint.lock().unwrap().clone(),
            state.hotkeys.lock().unwrap().clone(),
        )
    };
    let Some(manager) = manager else {
        return;
    };
    manager.clear();
    manager.register(ids::SHOW_HIDE, &settings.shortcut_keys.show_or_hide);
    manager.register(ids::PREV_GROUP, &settings.shortcut_keys.previous_group);
    manager.register(ids::NEXT_GROUP, &settings.shortcut_keys.next_group);
    if settings.shortcut_keys.quick_paste_enable {
        for i in 1..10 {
            manager.register(
                ids::QUICK_PASTE_1 + (i - 1),
                &format!("{}+{}", settings.shortcut_keys.quick_paste, i),
            );
        }
    }
    for (key, shortcut) in appoint.iter() {
        if let Ok(id) = key.parse::<i64>() {
            manager.register(ids::APPOINT_BASE + (id % 1000) as i32, &shortcut.shortcut);
        }
    }
}

/// Play a short preview of the selected sound.
pub fn preview_sound(cx: &App, copy: bool) {
    let s = cx.global::<AppState>().settings();
    let (kind, url) = crate::state::sound_for(&s, copy);
    if copy {
        sound::play_copy(kind, &url);
    } else {
        sound::play_paste(kind, &url);
    }
}

/// Handle one hotkey activation on the UI thread.
pub fn on_hotkey(id: i32, cx: &mut App) {
    match id {
        ids::SHOW_HIDE => windows::toggle_panel(cx),
        ids::PREV_GROUP => group_hotkey(false, cx),
        ids::NEXT_GROUP => group_hotkey(true, cx),
        id if (ids::QUICK_PASTE_1..ids::QUICK_PASTE_1 + 9).contains(&id) => {
            quick_paste(cx, (id - ids::QUICK_PASTE_1 + 1) as usize);
        }
        _ => appoint_paste(cx, (id - ids::APPOINT_BASE) as i64),
    }
}

fn group_hotkey(forward: bool, cx: &mut App) {
    let open = cx
        .global::<crate::windows::Registry>()
        .0
        .lock()
        .unwrap()
        .panel
        .is_some();
    if !open {
        windows::show_panel(cx);
    }
    windows::update_panel(cx, |view, _window, cx| {
        view.move_group_from_hotkey(forward, cx);
    });
}

/// `Ctrl+1..9` pastes the n-th entry of the history.
fn quick_paste(cx: &mut App, nth: usize) {
    let post = {
        let state = cx.global::<AppState>();
        state
            .query_posts(&PostQuery {
                status: Some(0),
                limit: Some(20),
                ..Default::default()
            })
            .get(nth.saturating_sub(1))
            .cloned()
    };
    if let Some(post) = post {
        clipassistant_core::paste::save_foreground_handle();
        cx.global::<AppState>().paste_post(&post, false);
    }
}

fn appoint_paste(cx: &mut App, slot: i64) {
    let target = {
        let state = cx.global::<AppState>();
        state
            .appoint
            .lock()
            .unwrap()
            .values()
            .find(|s| (s.id % 1000) == slot)
            .map(|s| s.id)
    };
    let Some(id) = target else { return };
    let post = cx
        .global::<AppState>()
        .query_posts(&PostQuery {
            id: Some(id),
            status: Some(0),
            ..Default::default()
        })
        .into_iter()
        .next();
    if let Some(post) = post {
        clipassistant_core::paste::save_foreground_handle();
        cx.global::<AppState>().paste_post(&post, false);
    }
}
