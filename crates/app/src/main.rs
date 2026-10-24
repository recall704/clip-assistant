//! 剪贴助手 — Rust + GPUI reimplementation of the Electron clipboard manager.
//!
//! Start-up sequence mirrors the original main process:
//! 1. open the JSON store and the SQLite database,
//! 2. install the settings-driven hotkeys, tray icon and auto-start entry,
//! 3. start the clipboard listener,
//! 4. show the panel when the show/hide hotkey fires.

// A clipboard manager is a desktop app: never open a console window. Logs still
// reach a redirected stderr.
#![windows_subsystem = "windows"]

mod actions;
mod state;
mod tray;
mod ui;
mod windows;

use gpui::{div, prelude::*, App, Application, Context, Render, Window, WindowOptions};
use clipassistant_core::clipboard::ClipboardMonitor;
use clipassistant_core::hotkey::HotkeyManager;
use clipassistant_core::model::Post;
use state::{prune_history, AppState};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// A hidden window that keeps the process alive when the panel is dismissed.
///
/// GPUI terminates the event loop when the last window closes, so the app keeps
/// this invisible window registered, like the original tray-resident process.
struct KeepAlive;

impl Render for KeepAlive {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

static MONITOR: OnceLock<ClipboardMonitor> = OnceLock::new();

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let state = match AppState::new() {
        Ok(state) => state,
        Err(err) => {
            eprintln!("初始化失败: {err}");
            std::process::exit(1);
        }
    };
    let settings = state.settings();

    Application::new().run(move |cx: &mut App| {
        cx.set_global(state);
        windows::init(cx);
        ui::text_input::bind_keys(cx);
        ui::panel::bind_keys(cx);

        // Hidden keep-alive window.
        if let Err(err) = cx.open_window(
            WindowOptions {
                show: false,
                focus: false,
                ..Default::default()
            },
            |_, cx| cx.new(|_| KeepAlive),
        ) {
            log::error!("无法创建后台窗口: {err}");
        }

        // --- global hotkeys -------------------------------------------------
        let hotkey_queue: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));
        let queue_for_hotkeys = hotkey_queue.clone();
        match HotkeyManager::new(move |id| {
            queue_for_hotkeys.lock().unwrap().push(id);
        }) {
            Some(manager) => {
                *cx.global::<AppState>().hotkeys.lock().unwrap() = Some(manager);
                actions::register_hotkeys(cx);
            }
            None => log::error!("无法初始化全局快捷键"),
        }

        // --- clipboard monitor ----------------------------------------------
        let clipboard_ticks = Arc::new(AtomicU32::new(0));
        let ticks = clipboard_ticks.clone();
        if let Some(monitor) = ClipboardMonitor::start(move || {
            ticks.fetch_add(1, Ordering::SeqCst);
        }) {
            let _ = MONITOR.set(monitor);
        } else {
            log::error!("无法启动剪贴板监听");
        }

        // --- auto start + history retention ---------------------------------
        clipassistant_core::autostart::set_enabled(settings.general.auto_open);
        prune_history(cx.global::<AppState>());

        // --- tray ------------------------------------------------------------
        tray::start();
        if !settings.general.menu_icon {
            tray::set_visible(false);
        }

        // --- background event pump -------------------------------------------
        let hk_queue = hotkey_queue.clone();
        let clip_ticks = clipboard_ticks.clone();
        cx.spawn(async move |cx| {
            let mut idle_ticks = 0u32;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let hotkeys: Vec<i32> = {
                    let mut guard = hk_queue.lock().unwrap();
                    std::mem::take(&mut *guard)
                };
                let captures = clip_ticks.swap(0, Ordering::SeqCst);
                let tray_events = tray::drain();

                let idle = hotkeys.is_empty() && captures == 0 && tray_events.is_empty();
                if idle {
                    idle_ticks += 1;
                    if idle_ticks >= 1500 {
                        idle_ticks = 0;
                        cx.update(|cx| prune_history(cx.global::<AppState>())).ok();
                    }
                } else {
                    idle_ticks = 0;
                }

                cx.update(move |cx| {
                    // All window/entity work is deferred: `cx.update` here is a
                    // nested App update on the async task, and creating or
                    // activating a window from inside it can re-enter GPUI's
                    // entity lease and abort the process.
                    if windows::panel_is_open(cx)
                        && windows::panel_focus_settled()
                        && !windows::panel_has_focus()
                    {
                        cx.defer(|cx| windows::hide_panel(cx));
                    }
                    for event in tray_events {
                        cx.defer(move |cx| handle_tray(event, cx));
                    }
                    for id in hotkeys {
                        cx.defer(move |cx| actions::on_hotkey(id, cx));
                    }
                    if captures > 0 {
                        let mut created: Option<Post> = None;
                        for _ in 0..captures {
                            let post = cx.global::<AppState>().capture_clipboard();
                            if post.is_some() {
                                created = post;
                            }
                        }
                        if created.is_some() && windows::panel_is_open(cx) {
                            cx.defer(|cx| refresh_panel(cx));
                        }
                    }
                })
                .ok();
            }
        })
        .detach();

        // --- first window -----------------------------------------------------
        // `CLIPASSISTANT_SHOW_PANEL` opens the clipboard panel immediately, which makes
        // the UI easy to inspect without a global hotkey.
        if std::env::var("CLIPASSISTANT_SHOW_PANEL").is_ok() {
            windows::show_panel(cx);
        }
        // `CLIPASSISTANT_FAKE_HOTKEY` replays the show/hide hotkey after a short delay;
        // useful to exercise the real hotkey code path without SendKeys.
        if std::env::var("CLIPASSISTANT_FAKE_HOTKEY").is_ok() {
            let queue = hotkey_queue.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(1500))
                    .await;
                queue
                    .lock()
                    .unwrap()
                    .push(clipassistant_core::hotkey::ids::SHOW_HIDE);
                cx.update(|_| ()).ok();
            })
            .detach();
        }
        let skip_settings = std::env::var("CLIPASSISTANT_SKIP_SETTINGS").is_ok();
        if !skip_settings && !settings.general.min_tray {
            windows::open_settings(cx);
        }
    });
}

fn handle_tray(event: tray::TrayEvent, cx: &mut App) {
    match event {
        tray::TrayEvent::TogglePanel => windows::toggle_panel(cx),
        tray::TrayEvent::OpenSettings => windows::open_settings_at_cursor(cx),
        tray::TrayEvent::PreviousGroup => actions::on_hotkey(clipassistant_core::hotkey::ids::PREV_GROUP, cx),
        tray::TrayEvent::NextGroup => actions::on_hotkey(clipassistant_core::hotkey::ids::NEXT_GROUP, cx),
        tray::TrayEvent::Guide => clipassistant_core::winutil::open_with_shell(
            "https://yuyuanweb.yuque.com/org-wiki-yuyuanweb-zvq1bg/dbtzac/syb2hz5gyyqsxtbf",
        ),
        tray::TrayEvent::Quit => cx.quit(),
    }
}

/// Tell the panel window to re-read the database.
fn refresh_panel(cx: &mut App) {
    windows::update_panel(cx, |view, _window, cx| view.refresh(cx));
}

/// Fetch the update announcement.
pub fn api_update_notice() -> Option<(String, String)> {
    let api = clipassistant_core::api::Api::new();
    let data = api.display_get("updateAnnouncement").ok()?;
    let value = data.get("value").cloned().unwrap_or_default();
    let raw = value
        .get("data")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| value.get("data").map(|v| v.to_string()))?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let title = parsed
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("更新公告")
        .to_string();
    let content = parsed
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("检测到新的版本更新。")
        .to_string();
    Some((title, content))
}

