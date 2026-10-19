//! Screen geometry helpers (`init/layout.ts`).
//!
//! The panel docks to the bottom edge (horizontal layout, 340px tall) or the
//! right edge (vertical layout, 290px wide) of the display under the cursor.

use crate::settings::Layout;

/// A rectangle on the virtual desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Height of the horizontal (bottom docked) panel.
pub const PANEL_HORIZONTAL_HEIGHT: i32 = 340;
/// Width of the vertical (right docked) panel.
pub const PANEL_VERTICAL_WIDTH: i32 = 290;

/// Bounds of the monitor that currently contains the mouse cursor.
pub fn monitor_bounds_at_cursor() -> Rect {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    unsafe {
        let pt = POINT { x: 0, y: 0 };
        let mut cursor = pt;
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor);        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let r = info.rcMonitor;
            return Rect {
                x: r.left,
                y: r.top,
                w: r.right - r.left,
                h: r.bottom - r.top,
            };
        }
        Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }
    }
}

/// Display scale factor (DPI / 96) of the monitor under the cursor.
///
/// GPUI lays windows out in logical pixels, so a physical rectangle has to be
/// divided by this value before it is handed to the windowing layer.
pub fn monitor_scale_at_cursor() -> f32 {
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let pt = windows::Win32::Foundation::POINT { x: 0, y: 0 };
        let mut cursor = pt;
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok() && dpi_x > 0 {
            return dpi_x as f32 / 96.0;
        }
        1.0
    }
}

/// Where the panel should sit for the given layout.
pub fn panel_geometry(layout: Layout) -> Rect {
    panel_geometry_in(monitor_bounds_at_cursor(), layout)
}

/// Dock the panel inside an explicit monitor rectangle.
///
/// The rectangle must be in the same unit the windowing layer expects (GPUI
/// works in logical pixels, so callers on Windows divide the physical rect by
/// the monitor scale factor first).
pub fn panel_geometry_in(monitor: Rect, layout: Layout) -> Rect {
    let b = monitor;
    match layout {
        Layout::Vertical => Rect {
            x: b.x + b.w - PANEL_VERTICAL_WIDTH,
            y: b.y,
            w: PANEL_VERTICAL_WIDTH,
            h: b.h,
        },
        Layout::Horizontal => Rect {
            x: b.x,
            y: b.y + b.h - PANEL_HORIZONTAL_HEIGHT,
            w: b.w,
            h: PANEL_HORIZONTAL_HEIGHT,
        },
    }
}

/// Size of the main clipboard panel for a layout (used by the UI layer).
pub fn panel_size(layout: Layout) -> (i32, i32) {
    let g = panel_geometry(layout);
    (g.w, g.h)
}

/// Centre a window of `w`x`h` on the monitor under the cursor.
pub fn centered(w: i32, h: i32) -> Rect {
    let b = monitor_bounds_at_cursor();
    Rect {
        x: b.x + (b.w - w) / 2,
        y: b.y + (b.h - h) / 2,
        w,
        h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_docks_to_bottom() {
        let g = panel_geometry(Layout::Horizontal);
        assert_eq!(g.h, PANEL_HORIZONTAL_HEIGHT);
        assert_eq!(g.y + g.h, monitor_bounds_at_cursor().y + monitor_bounds_at_cursor().h);
    }

    #[test]
    fn vertical_docks_to_right() {
        let g = panel_geometry(Layout::Vertical);
        assert_eq!(g.w, PANEL_VERTICAL_WIDTH);
        assert_eq!(g.x + g.w, monitor_bounds_at_cursor().x + monitor_bounds_at_cursor().w);
    }
}
