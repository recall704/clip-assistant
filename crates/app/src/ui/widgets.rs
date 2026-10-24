//! Small reusable widgets shared by the panel and the settings window.

use super::theme::Palette;
use gpui::{div, prelude::*, px, AnyElement, Hsla, IntoElement, SharedString, Window};

/// A switch / toggle (`toggle toggle-xs` in the original).
pub fn switch(
    id: impl Into<SharedString>,
    checked: bool,
    palette: &Palette,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let track = if checked {
        palette.primary
    } else if palette.dark {
        gpui::rgb(0x4A4A52)
    } else {
        gpui::rgb(0xD1D5DB)
    };
    div()
        .id(id)
        .w(px(34.))
        .h(px(18.))
        .flex_none()
        .rounded_full()
        .bg(track)
        .p(px(2.))
        .flex()
        .items_center()
        .cursor_pointer()
        .when(checked, |d| d.justify_end())
        .child(div().size(px(14.)).rounded_full().bg(gpui::rgb(0xFFFFFF)))
        .on_click(on_toggle)
}

/// A radio button (`radio radio-xs`).
pub fn radio(
    id: impl Into<SharedString>,
    checked: bool,
    palette: &Palette,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let inner = if checked { palette.primary } else { palette.border };
    div()
        .id(id)
        .size(px(16.))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(if checked { palette.primary } else { palette.muted })
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(div().size(px(8.)).rounded_full().bg(inner))
        .on_click(on_click)
}

/// A white rounded section with a title, matching the settings cards.
pub fn section(
    title: impl Into<SharedString>,
    palette: &Palette,
    children: Vec<AnyElement>,
) -> AnyElement {
    let title: SharedString = title.into();
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .w_full()
        .p(px(16.))
        .rounded(px(12.))
        .bg(palette.surface)
        .child(
            div()
                .text_size(px(13.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(palette.font)
                .child(title),
        )
        .children(children)
        .into_any_element()
}

/// One labelled row: a description on the left, a control on the right.
pub fn field(
    label: impl Into<SharedString>,
    palette: &Palette,
    control: impl IntoElement,
) -> AnyElement {
    let label: SharedString = label.into();
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .w_full()
        .py(px(3.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(palette.font)
                .child(label),
        )
        .child(control)
        .into_any_element()
}

/// Filled accent button.
pub fn primary_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    palette: &Palette,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let label: SharedString = label.into();
    let primary = palette.primary;
    let hover_bg: Hsla = primary.into();
    let hover_bg = hover_bg.opacity(0.9);
    div()
        .id(id)
        .px(px(14.))
        .py(px(4.))
        .rounded(px(8.))
        .bg(primary)
        .text_size(px(12.))
        .text_color(palette.primary_fg)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .child(label)
        .on_click(on_click)
}

/// Muted button used for secondary actions.
pub fn ghost_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    palette: &Palette,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let label: SharedString = label.into();
    let input = palette.input;
    div()
        .id(id)
        .px(px(12.))
        .py(px(4.))
        .rounded(px(8.))
        .bg(input)
        .text_size(px(12.))
        .text_color(palette.font)
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .child(label)
        .on_click(on_click)
}

/// A discrete slider rendered as clickable segments (used for the retention
/// window: 天/周/月/季度/半年/年/无限制).
pub fn step_slider(
    values: Vec<(String, String)>,
    selected: usize,
    palette: &Palette,
    on_select: impl Fn(usize, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let on_select = std::rc::Rc::new(on_select);
    let count = values.len();
    let primary = palette.primary;
    let track = palette.border;
    let mut track_row = div().flex().flex_row().w_full().h(px(22.)).rounded_full().bg(track);
    for (i, _) in values.iter().enumerate() {
        let cb = on_select.clone();
        let mut seg = div()
            .id(SharedString::from(format!("seg-{i}")))
            .flex_1()
            .h_full()
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center();
        if i == 0 {
            seg = seg.rounded_l(px(11.));
        }
        if i + 1 == count {
            seg = seg.rounded_r(px(11.));
        }
        if i == selected {
            seg = seg.bg(primary);
        }
        track_row = track_row.child(seg.on_click(move |_, window, cx| cb(i, window, cx)));
    }
    let labels = values
        .into_iter()
        .map(|(_, label)| {
            div()
                .flex_1()
                .text_size(px(11.))
                .text_color(palette.muted)
                .child(label)
        })
        .collect::<Vec<_>>();
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .w_full()
        .child(track_row)
        .child(div().flex().flex_row().w_full().children(labels))
        .into_any_element()
}

/// A coloured dot for group names.
pub fn dot(color: Hsla) -> impl IntoElement {
    div().size(px(8.)).flex_none().rounded_full().bg(color)
}

/// Tooltip-style caption.
pub fn caption(text: impl Into<SharedString>, palette: &Palette) -> AnyElement {
    let text: SharedString = text.into();
    div()
        .text_size(px(11.))
        .text_color(palette.muted)
        .child(text)
        .into_any_element()
}

/// A thin separator.
pub fn divider(palette: &Palette) -> AnyElement {
    div().h(px(1.)).w_full().bg(palette.border).into_any_element()
}

/// Placeholder for an image that could not be decoded.
pub fn image_placeholder(text: impl Into<SharedString>, palette: &Palette) -> impl IntoElement {
    let text: SharedString = text.into();
    div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .text_size(px(12.))
        .text_color(palette.muted)
        .child(text)
}
