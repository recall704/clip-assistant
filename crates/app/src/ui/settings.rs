//! The settings window — four pages reachable from the side rail:
//! 剪贴板 / 快捷键 / 同步 / 通用设置.

use super::panel::{current_palette, format_shortcut};
use super::text_input::{InputStyle, TextInput};
use super::theme::Palette;
use super::widgets;
use crate::actions;
use crate::state::{prune_history, AppState};
use crate::windows;
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, KeyDownEvent, Render,
    SharedString, Window,
};
use clipassistant_core::model::ContentType;
use clipassistant_core::settings::{HistoryCache, Layout, SoundInfo, Theme};

const TABS: [&str; 3] = ["剪贴板", "快捷键", "通用设置"];

pub struct SettingsView {
    tab: usize,
    confirm_clear: bool,
    clearing: bool,
    toast: Option<(SharedString, std::time::Instant)>,
    copy_sound: Entity<TextInput>,
    paste_sound: Entity<TextInput>,
    focus: FocusHandle,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let palette = current_palette(cx, window);
        let style = InputStyle::from_palette(&palette);
        let copy_sound = cx.new(|cx| {
            TextInput::new("复制音效文件路径或网址", style, cx)
                .on_submit(|text, _window, cx| {
                    let info = SoundInfo {
                        name: text.rsplit(['/', '\\']).next().unwrap_or(text).to_string(),
                        size: 0,
                        url: text.to_string(),
                    };
                    actions::set_user_sound(cx, true, info);
                    actions::preview_sound(cx, true);
                })
        });
        let paste_sound = cx.new(|cx| {
            TextInput::new("粘贴音效文件路径或网址", style, cx)
                .on_submit(|text, _window, cx| {
                    let info = SoundInfo {
                        name: text.rsplit(['/', '\\']).next().unwrap_or(text).to_string(),
                        size: 0,
                        url: text.to_string(),
                    };
                    actions::set_user_sound(cx, false, info);
                    actions::preview_sound(cx, false);
                })
        });
        let s = cx.global::<AppState>().settings();
        copy_sound_sync(&copy_sound, &s.clipboard.sounds.user_copy.url, cx);
        copy_sound_sync(&paste_sound, &s.clipboard.sounds.user_paste.url, cx);
        SettingsView {
            tab: 0,
            confirm_clear: false,
            clearing: false,
            toast: None,
            copy_sound,
            paste_sound,
            focus: cx.focus_handle(),
        }
    }

    fn toast(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some((text.into(), std::time::Instant::now()));
        cx.notify();
    }

    // ------------------------------------------------------------------ rail

    fn rail(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut rail = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(96.))
            .h_full()
            .py(px(20.))
            .gap(px(6.))
            .items_center();
        for (i, name) in TABS.iter().enumerate() {
            let selected = self.tab == i;
            rail = rail.child(
                div()
                    .id(SharedString::from(format!("tab-{i}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(72.))
                    .h(px(34.))
                    .rounded(px(10.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(palette.surface).text_color(palette.primary))
                    .when(!selected, |d| d.text_color(palette.font))
                    .child(*name)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                        this.tab = i;
                        cx.notify();
                    })),
            );
        }
        rail.into_any_element()
    }

    // ----------------------------------------------------------- page: shear

    fn page_shear(&mut self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let s = cx.global::<AppState>().settings();
        let sounds = s.clipboard.sounds.clone();

        let sound_type_row = |kind: i64, label: &'static str| {
            widgets::field(
                label,
                palette,
                widgets::radio(
                    format!("sound-type-{kind}"),
                    sounds.sound_type == kind,
                    palette,
                    move |_, _window, cx| {
                        cx.defer(move |cx| actions::set_sound_type(cx, kind));
                    },
                ),
            )
            .into_any_element()
        };

        let sound_section = widgets::section(
            "音效",
            palette,
            vec![
                widgets::field(
                    "启用音效",
                    palette,
                    widgets::switch(
                        "sound-open",
                        sounds.open,
                        palette,
                        |_, _window, cx| {
                            cx.defer(|cx| {
                                let open = !cx.global::<AppState>().settings().clipboard.sounds.open;
                                actions::set_sound_open(cx, open);
                            });
                        },
                    ),
                ),
                sound_type_row(0, "默认音效"),
                sound_type_row(1, "小黑子音效"),
                sound_type_row(2, "自定义音效"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .pl(px(8.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(28.))
                                    .px(px(6.))
                                    .rounded(px(6.))
                                    .bg(palette.input)
                                    .child(self.copy_sound.clone()),
                            )
                            .child(widgets::ghost_button(
                                "preview-copy",
                                "试听",
                                palette,
                                cx.listener(|_this, _: &gpui::ClickEvent, _window, cx| {
                                    actions::preview_sound(cx, true);
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(28.))
                                    .px(px(6.))
                                    .rounded(px(6.))
                                    .bg(palette.input)
                                    .child(self.paste_sound.clone()),
                            )
                            .child(widgets::ghost_button(
                                "preview-paste",
                                "试听",
                                palette,
                                cx.listener(|_this, _: &gpui::ClickEvent, _window, cx| {
                                    actions::preview_sound(cx, false);
                                }),
                            )),
                    )
                    .child(widgets::caption(
                        "支持 mp3 / wav：填写本机路径或网址",
                        palette,
                    ))
                    .into_any_element(),
            ],
        );

        let theme = s.clipboard.theme;
        let theme_section = widgets::section(
            "主题色",
            palette,
            vec![div()
                .flex()
                .flex_row()
                .gap(px(18.))
                .children(
                    [
                        (Theme::Light, "浅色"),
                        (Theme::Dark, "深色"),
                        (Theme::System, "跟随系统"),
                    ]
                    .into_iter()
                    .map(|(value, label)| {
                        let selected = theme == value;
                        div()
                            .id(SharedString::from(format!("theme-{label}")))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .cursor_pointer()
                            .child(widgets::radio(
                                format!("theme-radio-{label}"),
                                selected,
                                palette,
                                move |_, _window, cx| {
                                    cx.defer(move |cx| actions::set_theme(cx, value));
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(if selected { palette.primary } else { palette.font })
                                    .child(label),
                            )
                            .into_any_element()
                    })
                    .collect::<Vec<_>>(),
                )
                .into_any_element()],
        );

        let layout = s.clipboard.layout;
        let layout_section = widgets::section(
            "布局",
            palette,
            vec![
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.))
                    .children(
                        [(Layout::Horizontal, "横向"), (Layout::Vertical, "纵向")]
                            .into_iter()
                            .map(|(value, label)| {
                                let selected = layout == value;
                                div()
                                    .id(SharedString::from(format!("layout-{label}")))
                                    .flex_1()
                                    .h(px(34.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(8.))
                                    .cursor_pointer()
                                    .when(selected, |d| {
                                        d.bg(palette.primary).text_color(palette.primary_fg)
                                    })
                                    .when(!selected, |d| d.bg(palette.input).text_color(palette.font))
                                    .text_size(px(12.))
                                    .child(label)
                                    .on_click(cx.listener(
                                        move |_this, _: &gpui::ClickEvent, _window, cx| {
                                            actions::set_layout(cx, value);
                                        },
                                    ))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .into_any_element(),
                widgets::caption(
                    "横向停靠在屏幕底部，纵向停靠在屏幕右侧",
                    palette,
                ),
            ],
        );

        div()
            .flex()
            .flex_row()
            .gap(px(14.))
            .size_full()
            .children(vec![
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .flex_1()
                    .child(sound_section)
                    .into_any_element(),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .w(px(300.))
                    .child(theme_section)
                    .child(layout_section)
                    .into_any_element(),
            ])
            .into_any_element()
    }

    // -------------------------------------------------------- page: shortcut

    fn page_shortcut(&mut self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let s = cx.global::<AppState>().settings();
        let hotkeys = s.shortcut_keys.clone();

        let show_section = widgets::section(
            "唤出/隐藏剪贴板",
            palette,
            vec![widgets::field(
                "快捷键",
                palette,
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(shortcut_box(
                        "sc-show",
                        &hotkeys.show_or_hide,
                        palette,
                        cx,
                        |this, key, cx| {
                            this.apply_show_hide(key, cx);
                        },
                    ))
                    .child(widgets::ghost_button(
                        "sc-show-reset",
                        "重置",
                        palette,
                        cx.listener(|_this, _: &gpui::ClickEvent, _window, cx| {
                            cx.defer(|cx| {
                                actions::set_show_or_hide(cx, "ctrl+`");
                            });
                        }),
                    ))
                    .into_any_element(),
            )],
        );

        let group_section = widgets::section(
            "切换分组",
            palette,
            vec![
                widgets::field(
                    "切换上一组",
                    palette,
                    shortcut_box(
                        "sc-prev",
                        &hotkeys.previous_group,
                        palette,
                        cx,
                        |this, key, cx| this.apply_prev_group(key, cx),
                    )
                    .into_any_element(),
                ),
                widgets::divider(palette).into_any_element(),
                widgets::field(
                    "切换下一组",
                    palette,
                    shortcut_box(
                        "sc-next",
                        &hotkeys.next_group,
                        palette,
                        cx,
                        |this, key, cx| this.apply_next_group(key, cx),
                    )
                    .into_any_element(),
                ),
            ],
        );

        let prefixes: Vec<String> = vec!["ctrl", "shift", "alt", "ctrl+shift", "ctrl+alt"]
            .into_iter()
            .map(String::from)
            .collect();
        let quick_section = widgets::section(
            "快速粘贴",
            palette,
            vec![
                widgets::field(
                    "开启快速粘贴",
                    palette,
                    widgets::switch(
                        "quick-enable",
                        hotkeys.quick_paste_enable,
                        palette,
                        |_, _window, cx| {
                            cx.defer(|cx| {
                                let on = cx
                                    .global::<AppState>()
                                    .settings()
                                    .shortcut_keys
                                    .quick_paste_enable;
                                actions::set_quick_paste_enabled(cx, !on);
                            });
                        },
                    ),
                ),
                widgets::field(
                    "快速粘贴前缀",
                    palette,
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(6.))
                        .children(
                            prefixes
                                .iter()
                                .map(|p| {
                                    let p = p.clone();
                                    let selected = hotkeys.quick_paste == p;
                                    div()
                                        .id(SharedString::from(format!("qp-{p}")))
                                        .px(px(8.))
                                        .py(px(3.))
                                        .rounded(px(6.))
                                        .text_size(px(11.))
                                        .cursor_pointer()
                                        .when(selected, |d| {
                                            d.bg(palette.primary).text_color(palette.primary_fg)
                                        })
                                        .when(!selected, |d| {
                                            d.bg(palette.input).text_color(palette.font)
                                        })
                                        .child(SharedString::from(p.clone()))
                                        .on_click(cx.listener(
                                            move |_this, _: &gpui::ClickEvent, _window, cx| {
                                                let value = p.clone();
                                                cx.defer(move |cx| {
                                                    actions::set_quick_paste(cx, &value)
                                                });
                                            },
                                        ))
                                        .into_any_element()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.muted)
                                .child("+ 数字1～9"),
                        )
                        .into_any_element(),
                ),
            ],
        );

        let state = cx.global::<AppState>();
        let appoints: Vec<(i64, i64, String)> = state
            .appoint
            .lock()
            .unwrap()
            .values()
            .map(|s| (s.id, s.content_type, s.shortcut.clone()))
            .collect();
        let mut list = div().flex().flex_col().gap(px(8.));
        if appoints.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(80.))
                    .text_size(px(12.))
                    .text_color(palette.muted)
                    .child("您还没有为内容指定过快捷键"),
            );
        } else {
            for (id, ct, shortcut) in appoints {
                let label = ContentType::from_i64(ct).label();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("appoint-{id}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .bg(palette.input)
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(palette.font)
                                .child(format!("{label} · {id}")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .child(shortcut_box(
                                    format!("appoint-box-{id}"),
                                    &shortcut,
                                    palette,
                                    cx,
                                    move |this, key, cx| this.apply_appoint(id, key, cx),
                                ))
                                .child(widgets::ghost_button(
                                    format!("appoint-del-{id}"),
                                    "删除",
                                    palette,
                                    move |_, _window, cx| {
                                        cx.global::<AppState>().store.remove_hot_key(&id.to_string());
                                        cx.global::<AppState>().reload_appoint();
                                        actions::register_hotkeys(cx);
                                        actions::notify_all(cx);
                                    },
                                )),
                        ),
                );
            }
        }
        let appoint_section = widgets::section(
            "指定内容快捷键",
            palette,
            vec![list.into_any_element()],
        );

        div()
            .flex()
            .flex_row()
            .gap(px(14.))
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .w(px(360.))
                    .child(show_section)
                    .child(group_section)
                    .child(quick_section),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .child(appoint_section),
            )
            .into_any_element()
    }

    fn apply_show_hide(&mut self, key: &str, cx: &mut Context<Self>) {
        let key = key.to_string();
        actions::set_show_or_hide(cx, &key);
        self.toast("快捷键已更新", cx);
    }

    fn apply_prev_group(&mut self, key: &str, cx: &mut Context<Self>) {
        let key = key.to_string();
        actions::set_previous_group(cx, &key);
        self.toast("快捷键已更新", cx);
    }

    fn apply_next_group(&mut self, key: &str, cx: &mut Context<Self>) {
        let key = key.to_string();
        actions::set_next_group(cx, &key);
        self.toast("快捷键已更新", cx);
    }

    fn apply_appoint(&mut self, post_id: i64, key: &str, cx: &mut Context<Self>) {
        let _ = post_id;
        let _ = key;
        actions::register_hotkeys(cx);
        self.toast("快捷键已更新", cx);
    }

    // --------------------------------------------------------- page: general

    fn page_general(&mut self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let s = cx.global::<AppState>().settings();
        let general = s.general.clone();
        let labels = [
            ("day", "天"),
            ("week", "周"),
            ("month", "月"),
            ("quarter", "季度"),
            ("half", "半年"),
            ("year", "年"),
            ("infinity", "无限制"),
        ];
        let options: Vec<(String, String)> = labels
            .iter()
            .map(|(v, l)| (v.to_string(), l.to_string()))
            .collect();
        let selected = labels
            .iter()
            .position(|(v, _)| *v == general.history_cache.as_str())
            .unwrap_or(6);

        let startup = widgets::section(
            "启动设置",
            palette,
            vec![
                widgets::field(
                    "开机时自动运行",
                    palette,
                    widgets::switch(
                        "auto-run",
                        general.auto_open,
                        palette,
                        |_, _window, cx| {
                            cx.defer(|cx| {
                                let current =
                                    cx.global::<AppState>().settings().general.auto_open;
                                actions::set_auto_run(cx, !current);
                            });
                        },
                    ),
                ),
                widgets::field(
                    "启动时最小化到托盘",
                    palette,
                    widgets::switch(
                        "min-tray",
                        general.min_tray,
                        palette,
                        |_, _window, cx| {
                            cx.defer(|cx| {
                                let current =
                                    cx.global::<AppState>().settings().general.min_tray;
                                actions::set_min_tray(cx, !current);
                            });
                        },
                    ),
                ),
                widgets::field(
                    "显示菜单栏图标",
                    palette,
                    widgets::switch(
                        "menu-icon",
                        general.menu_icon,
                        palette,
                        |_, _window, cx| {
                            cx.defer(|cx| {
                                let current =
                                    cx.global::<AppState>().settings().general.menu_icon;
                                actions::set_menu_icon(cx, !current);
                            });
                        },
                    ),
                ),
            ],
        );

        let history = widgets::section(
            "历史记录容量设置",
            palette,
            vec![
                widgets::step_slider(
                    options,
                    selected,
                    palette,
                    move |index: usize, _window: &mut Window, cx: &mut App| {
                        let cache = HistoryCache::from_slider((index as i64) * 20);
                        cx.defer(move |cx| actions::set_history_cache(cx, cache));
                    },
                ),
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .child(widgets::caption("仅清除本地历史记录", palette))
                    .child(widgets::ghost_button(
                        "clear-history",
                        "清空历史记录",
                        palette,
                        cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                            this.confirm_clear = true;
                            cx.notify();
                        }),
                    ))
                    .into_any_element(),
            ],
        );

        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .size_full()
            .child(startup)
            .child(history)
            .into_any_element()
    }

    fn body(&mut self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let page = match self.tab {
            0 => self.page_shear(palette, cx),
            1 => self.page_shortcut(palette, cx),
            _ => self.page_general(palette, cx),
        };
        div()
            .id("settings-body")
            .flex()
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .p(px(16.))
            .rounded(px(14.))
            .bg(palette.backdrop)
            .overflow_y_scroll()
            .child(page)
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = current_palette(cx, window);

        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(palette.surface)
            .track_focus(&self.focus)
            .child(
                // Header
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(56.))
                    .px(px(18.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_baseline()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(palette.primary)
                                    .child(clipassistant_core::APP_NAME),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(palette.muted)
                                    .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .child(widgets::ghost_button(
                                "settings-min",
                                "—",
                                &palette,
                                cx.listener(|_this, _: &gpui::ClickEvent, window, _cx| {
                                    window.minimize_window();
                                }),
                            ))
                            .child(widgets::ghost_button(
                                "settings-close",
                                "✕",
                                &palette,
                                cx.listener(|_this, _: &gpui::ClickEvent, _window, cx| {
                                    windows::close_settings(cx);
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.))
                    .gap(px(6.))
                    .px(px(10.))
                    .pb(px(10.))
                    .child(self.rail(&palette, cx))
                    .child(self.body(&palette, cx)),
            );

        if let Some((text, at)) = self.toast.clone() {
            if at.elapsed() < std::time::Duration::from_millis(2400) {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .right(px(0.))
                        .top(px(64.))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px(px(12.))
                                .py(px(6.))
                                .rounded(px(8.))
                                .bg(palette.primary)
                                .text_size(px(12.))
                                .text_color(palette.primary_fg)
                                .child(text),
                        ),
                );
            }
        }

        if self.confirm_clear {
            root = root.child(
                div()
                    .id("confirm-clear")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgba(0x00000060))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .p(px(18.))
                            .rounded(px(12.))
                            .w(px(300.))
                            .bg(palette.surface)
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(palette.font)
                                    .child("是否要清空所有历史记录？该操作不可逆！"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .gap(px(8.))
                                    .child(widgets::ghost_button(
                                        "clear-cancel",
                                        "取消",
                                        &palette,
                                        cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                                            this.confirm_clear = false;
                                            cx.notify();
                                        }),
                                    ))
                                    .child(widgets::primary_button(
                                        "clear-ok",
                                        "确认",
                                        &palette,
                                        cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                                            cx.global::<AppState>().clear_history();
                                            actions::register_hotkeys(cx);
                                            this.confirm_clear = false;
                                            this.clearing = true;
                                            let _ = prune_history(cx.global::<AppState>());
                                            this.toast("历史记录已清空", cx);
                                            actions::notify_all(cx);
                                        }),
                                    )),
                            ),
                    ),
            );
        }

        root.relative()
    }
}

fn copy_sound_sync(
    entity: &Entity<TextInput>,
    value: &str,
    cx: &mut App,
) {
    entity.update(cx, |input, cx| input.set_content(value, cx));
}

/// A key-capture box, equivalent to the original `HotKeyBoard` component.
fn shortcut_box(
    id: impl Into<SharedString>,
    value: &str,
    palette: &Palette,
    cx: &mut Context<SettingsView>,
    on_change: impl Fn(&mut SettingsView, &str, &mut Context<SettingsView>) + 'static,
) -> AnyElement {
    let id: SharedString = id.into();
    let value: SharedString = value.to_string().into();
    let on_change = std::rc::Rc::new(on_change);
    div()
        .id(id)
        .min_w(px(150.))
        .h(px(26.))
        .px(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .border_1()
        .border_color(palette.border)
        .bg(palette.input)
        .text_size(px(11.))
        .text_color(if value.is_empty() {
            palette.muted
        } else {
            palette.font
        })
        .child(if value.is_empty() {
            SharedString::from("请输入快捷键")
        } else {
            value
        })
        .on_key_down(cx.listener(
            move |this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key == "escape" {
                    on_change(this, "", cx);
                    return;
                }
                if let Some(formatted) = format_shortcut(&event.keystroke) {
                    on_change(this, &formatted, cx);
                }
            },
        ))
        .into_any_element()
}
