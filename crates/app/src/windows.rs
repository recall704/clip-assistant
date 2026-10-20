//! Window registry: opening, closing and positioning the three windows.
//!
//! `WindowHandle<V>` is not `Copy`, so every accessor takes the handle out of
//! the registry, uses it and puts it back when the window is still alive.

use crate::ui::{panel::PanelView, settings::SettingsView, update::UpdateView};
use gpui::{
    px, App, AppContext as _, Bounds, Context, DisplayId, Window, WindowBackgroundAppearance,
    WindowHandle, WindowKind, WindowOptions,
};
use clipassistant_core::layout::{self, Rect};
use std::sync::Mutex;

#[derive(Default)]
pub struct Windows {
    pub panel: Option<WindowHandle<PanelView>>,
    pub settings: Option<WindowHandle<SettingsView>>,
    pub update: Option<WindowHandle<UpdateView>>,
}

pub struct Registry(pub Mutex<Windows>);
impl gpui::Global for Registry {}

pub fn init(cx: &mut App) {
    cx.set_global(Registry(Mutex::new(Windows::default())));
}

fn with_registry<R>(cx: &App, f: impl FnOnce(&mut Windows) -> R) -> R {
    let registry = cx.global::<Registry>();
    let mut guard = registry.0.lock().unwrap();
    f(&mut guard)
}

fn bounds_of(r: Rect) -> Bounds<gpui::Pixels> {
    Bounds {
        origin: gpui::point(px(r.x as f32), px(r.y as f32)),
        size: gpui::size(px(r.w as f32), px(r.h as f32)),
    }
}

/// A GPUI display rectangle in the same integer units as [`Rect`].
fn rect_of(bounds: Bounds<gpui::Pixels>) -> Rect {
    Rect {
        x: f32::from(bounds.origin.x).round() as i32,
        y: f32::from(bounds.origin.y).round() as i32,
        w: f32::from(bounds.size.width).round() as i32,
        h: f32::from(bounds.size.height).round() as i32,
    }
}

fn panel_options(bounds: Bounds<gpui::Pixels>, display: Option<DisplayId>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
        display_id: display,
        titlebar: None,
        kind: WindowKind::PopUp,
        window_background: WindowBackgroundAppearance::Transparent,
        is_resizable: false,
        is_movable: false,
        focus: true,
        show: true,
        ..Default::default()
    }
}

fn dialog_options(bounds: Bounds<gpui::Pixels>, display: Option<DisplayId>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
        display_id: display,
        titlebar: None,
        kind: WindowKind::Normal,
        is_resizable: false,
        focus: true,
        show: true,
        ..Default::default()
    }
}

// ------------------------------------------------------------------- panel

/// Show or hide the clipboard panel.
pub fn toggle_panel(cx: &mut App) {
    if with_registry(cx, |w| w.panel.is_some()) {
        hide_panel(cx);
        // Hand focus back to the previous application without pasting.
        clipassistant_core::paste::focus_and_paste(None, false);
        return;
    }
    show_panel(cx);
}

/// Logical (GPUI) rectangle of the monitor under the mouse pointer.
///
/// Win32 reports monitor rectangles in physical pixels; GPUI expects logical
/// pixels, so the physical rectangle is divided by that display's scale factor.
fn logical_monitor_at_cursor() -> Rect {
    let physical = layout::monitor_bounds_at_cursor();
    let scale = layout::monitor_scale_at_cursor();
    if scale <= 0.0 || (scale - 1.0).abs() < f32::EPSILON {
        return physical;
    }
    Rect {
        x: (physical.x as f32 / scale).round() as i32,
        y: (physical.y as f32 / scale).round() as i32,
        w: (physical.w as f32 / scale).round() as i32,
        h: (physical.h as f32 / scale).round() as i32,
    }
}

/// Monitor under the mouse pointer as the window layer sees it.
struct CursorMonitor {
    /// Logical rectangle, taken from GPUI's own display rather than recomputed.
    bounds: Rect,
    /// Display the window must be created on; `None` only when the pointer is
    /// on a monitor GPUI does not list.
    display: Option<DisplayId>,
}

/// Resolve the monitor under the pointer to a GPUI display.
///
/// The display id is not cosmetic: GPUI creates a window on the primary display
/// when it is missing, and then discards the requested bounds when they do not
/// land on that display (its Windows backend falls back to `default_bounds` in
/// `retrieve_window_placement`). That is what used to turn a panel asked to
/// dock on a secondary monitor into a centred default-sized window on the
/// primary one.
fn cursor_monitor(cx: &App) -> CursorMonitor {
    let physical = logical_monitor_at_cursor();
    let displays = cx.displays();
    let monitors: Vec<Rect> = displays.iter().map(|d| rect_of(d.bounds())).collect();
    match layout::monitor_index_of(&monitors, physical) {
        Some(index) => CursorMonitor {
            bounds: monitors[index],
            display: Some(displays[index].id()),
        },
        None => CursorMonitor {
            bounds: physical,
            display: None,
        },
    }
}

/// HWND of the panel window, remembered when it is created.
///
/// The panel is raised and focused through raw Win32 calls only: doing it via
/// `Window::activate_window()` from inside an entity update would re-enter the
/// view (GPUI fires the activation observers synchronously) and panic.
static PANEL_HWND: Mutex<isize> = Mutex::new(0);

/// When the panel was last shown — used to give Windows a moment to hand over
/// the foreground before blur-to-hide starts watching.
static PANEL_SHOWN_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Remember the panel's HWND. Called from the window-creation closure.
fn remember_panel_hwnd(window: &Window) {
    // Fully qualified: `Window` also has an inherent `window_handle()` that
    // returns GPUI's own handle type and would shadow the trait method.
    let handle = raw_window_handle::HasWindowHandle::window_handle(window);
    let Ok(handle) = handle else {
        return;
    };
    if let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_raw() {
        *PANEL_HWND.lock().unwrap() = win32.hwnd.get() as isize;
    }
}

/// Raise the panel above every window and give it the real keyboard focus.
///
/// Pure Win32: it never borrows a GPUI entity, so it is safe to call at any
/// point. GPUI learns about the activation from `WM_ACTIVATE` / `WM_SETFOCUS`.
pub fn focus_panel() {
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let raw = windows::Win32::Foundation::HWND(*PANEL_HWND.lock().unwrap() as *mut _);
    if raw.0.is_null() || !clipassistant_core::clipboard::hwnd_is_valid(raw) {
        return;
    }
    unsafe {
        let _ = SetWindowPos(
            raw,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    // Foreground ownership (the helper retries through `AttachThreadInput`
    // when the foreground lock refuses), then the keyboard focus itself.
    let foreground = clipassistant_core::winutil::set_foreground_window(raw);
    unsafe {
        let _ = SetFocus(Some(raw));
    }
    log::debug!("面板置前: SetForegroundWindow={foreground}");
}

/// Open the panel at the dock position for the configured layout.
pub fn show_panel(cx: &mut App) {
    let layout = cx
        .global::<crate::state::AppState>()
        .settings()
        .clipboard
        .layout;
    let monitor = cursor_monitor(cx);
    let rect = layout::panel_geometry_in(monitor.bounds, layout);
    log::debug!(
        "面板停靠位置(逻辑像素): {rect:?} display={:?}",
        monitor.display.map(u32::from)
    );
    clipassistant_core::paste::save_foreground_handle();
    let options = panel_options(bounds_of(rect), monitor.display);
    if let Ok(handle) = cx.open_window(options, |window, cx| {
        remember_panel_hwnd(window);
        cx.new(|cx| PanelView::new(window, cx))
    }) {
        with_registry(cx, |w| w.panel = Some(handle));
        *PANEL_SHOWN_AT.lock().unwrap() = Some(std::time::Instant::now());
        // The HWND only becomes visible after the root view is built, and
        // Windows refuses to foreground an invisible window — so the focus is
        // claimed once this update finishes …
        cx.defer(|_cx| focus_panel());
        // … and once more shortly after, because the first activation of a
        // freshly created window is sometimes dropped.
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(180))
                .await;
            cx.update(|_cx| focus_panel()).ok();
        })
        .detach();
    }
}

/// Whether the panel window currently owns the foreground (and therefore the
/// keyboard). Returns `true` when there is no panel, so callers never hide a
/// window that does not exist.
pub fn panel_has_focus() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GetForegroundWindow, GA_ROOT};
    let raw = windows::Win32::Foundation::HWND(*PANEL_HWND.lock().unwrap() as *mut _);
    if raw.0.is_null() || !clipassistant_core::clipboard::hwnd_is_valid(raw) {
        return true;
    }
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground == raw {
            return true;
        }
        // Normalise child/owned windows (e.g. the IME candidate window).
        GetAncestor(foreground, GA_ROOT) == raw
    }
}

/// Give Windows a moment to hand the foreground over after showing.
pub fn panel_focus_settled() -> bool {
    PANEL_SHOWN_AT
        .lock()
        .unwrap()
        .map(|at| at.elapsed().as_millis() > 400)
        .unwrap_or(false)
}

/// Forget the panel without removing its window: for views that close
/// themselves (`window.remove_window()`), so the registry never keeps a stale
/// handle.
pub fn forget_panel(cx: &mut App) {
    let _ = with_registry(cx, |w| w.panel.take());
    *PANEL_HWND.lock().unwrap() = 0;
    *PANEL_SHOWN_AT.lock().unwrap() = None;
}

/// Close the panel without touching the foreground — used when the user clicks
/// another application, so their click is not stolen back.
pub fn hide_panel(cx: &mut App) {
    if let Some(handle) = with_registry(cx, |w| w.panel.take()) {
        handle
            .update(cx, |_, window, _| window.remove_window())
            .ok();
    }
    *PANEL_HWND.lock().unwrap() = 0;
    *PANEL_SHOWN_AT.lock().unwrap() = None;
}

/// Centre a fixed-size dialog inside a monitor rectangle.
fn centered_in(monitor: Rect, w: i32, h: i32) -> Rect {
    Rect {
        x: monitor.x + (monitor.w - w) / 2,
        y: monitor.y + (monitor.h - h) / 2,
        w,
        h,
    }
}

/// Re-dock the panel after a layout change.
pub fn redock_panel(cx: &mut App) {
    if let Some(handle) = with_registry(cx, |w| w.panel.take()) {
        handle
            .update(cx, |_, window, _| window.remove_window())
            .ok();
    }
    show_panel(cx);
}

/// Run `f` against the panel view when it is open.
pub fn update_panel<R>(
    cx: &mut App,
    f: impl FnOnce(&mut PanelView, &mut Window, &mut Context<PanelView>) -> R,
) -> Option<R> {
    let handle = with_registry(cx, |w| w.panel.take())?;
    match handle.update(cx, f) {
        Ok(result) => {
            with_registry(cx, |w| w.panel = Some(handle));
            Some(result)
        }
        Err(_) => None,
    }
}

pub fn panel_is_open(cx: &App) -> bool {
    with_registry(cx, |w| w.panel.is_some())
}

// ---------------------------------------------------------------- settings

pub fn open_settings(cx: &mut App) {
    if let Some(handle) = with_registry(cx, |w| w.settings.take()) {
        handle
            .update(cx, |_, window, _| window.activate_window())
            .ok();
        with_registry(cx, |w| w.settings = Some(handle));
        return;
    }
    let monitor = cursor_monitor(cx);
    let rect = centered_in(monitor.bounds, 810, 580);
    let options = dialog_options(bounds_of(rect), monitor.display);
    if let Ok(handle) = cx.open_window(options, |window, cx| {
        cx.new(|cx| SettingsView::new(window, cx))
    }) {
        with_registry(cx, |w| w.settings = Some(handle));
        // The original checks for a new release once the settings window opens.
        cx.spawn(async move |cx| {
            let notice = cx
                .background_executor()
                .spawn(async { crate::api_update_notice() })
                .await;
            if notice.is_some() {
                cx.update(|cx| open_update(cx)).ok();
            }
        })
        .detach();
    }
}

pub fn close_settings(cx: &mut App) {
    if let Some(handle) = with_registry(cx, |w| w.settings.take()) {
        handle
            .update(cx, |_, window, _| window.remove_window())
            .ok();
    }
}

// ------------------------------------------------------------------ update

pub fn open_update(cx: &mut App) {
    if with_registry(cx, |w| w.update.is_some()) {
        return;
    }
    let monitor = cursor_monitor(cx);
    let rect = centered_in(monitor.bounds, 320, 320);
    let mut options = dialog_options(bounds_of(rect), monitor.display);
    options.kind = WindowKind::PopUp;
    if let Ok(handle) = cx.open_window(options, |window, cx| {
        cx.new(|cx| UpdateView::new(window, cx))
    }) {
        with_registry(cx, |w| w.update = Some(handle));
    }
}

pub fn close_update(cx: &mut App) {
    if let Some(handle) = with_registry(cx, |w| w.update.take()) {
        handle
            .update(cx, |_, window, _| window.remove_window())
            .ok();
    }
}
