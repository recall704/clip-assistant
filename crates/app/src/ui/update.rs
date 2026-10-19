//! The auto-update modal window.

use super::panel::current_palette;
use super::widgets;
use crate::windows;
use gpui::{div, prelude::*, px, Context, Render, SharedString, Window};

pub struct UpdateView {
    content: SharedString,
    title: SharedString,
    channel: Option<SharedString>,
}

impl UpdateView {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = UpdateView {
            title: "更新公告".into(),
            content: "检测到新的版本更新，请下载最新版本后使用。".into(),
            channel: None,
        };
        cx.spawn(async move |this, cx| {
            let info = cx
                .background_executor()
                .spawn(async { crate::api_update_notice() })
                .await;
            this.update(cx, |this, cx| {
                if let Some((title, content)) = info {
                    this.title = title.into();
                    this.content = content.into();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        view
    }
}

impl Render for UpdateView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = current_palette(cx, window);
        let tip = self.channel.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .rounded(px(12.))
            .bg(palette.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(52.))
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(palette.primary)
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .id("update-body")
                    .flex_1()
                    .min_h(px(0.))
                    .px(px(16.))
                    .overflow_y_scroll()
                    .text_size(px(12.))
                    .text_color(palette.font)
                    .child(self.content.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .h(px(48.))
                    .child(widgets::primary_button(
                        "update-download",
                        "立即更新",
                        &palette,
                        cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                            this.channel = Some("高速下载".into());
                            clipassistant_core::winutil::open_with_shell("https://jianqiezhushou.com");
                            cx.notify();
                        }),
                    ))
                    .child(widgets::ghost_button(
                        "update-cancel",
                        "取消更新",
                        &palette,
                        cx.listener(|_this, _: &gpui::ClickEvent, _window, cx| {
                            windows::close_update(cx);
                        }),
                    ))
                    .when_some(tip, |d, tip| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.muted)
                                .child(tip),
                        )
                    }),
            )
    }
}
