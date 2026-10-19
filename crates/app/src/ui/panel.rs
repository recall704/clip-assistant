//! The clipboard panel — the app's main window.
//!
//! Mirrors the original renderer's `/clipboard` route: a 50px header with the
//! search box and the group strip, then the 250×250 cards of the active group.

use super::card::{render_card, CardHandlers};
use super::text_input::{InputStyle, TextInput};
use super::theme::{group_color, Palette};
use super::widgets;
use crate::state::AppState;
use crate::windows;
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, Focusable, KeyBinding,
    KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, Render, ScrollHandle,
    SharedString, Window, actions,
};
use clipassistant_core::db::PostPatch;
use clipassistant_core::model::{ContentType, Post, PostGroup, PostQuery, ALL_GROUP_NAME};
use clipassistant_core::settings::{AppointShortcut, Layout};
use std::rc::Rc;

actions!(clipboard_panel, [FocusSearch]);

/// Key bindings owned by the clipboard panel. Call once at start-up.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "ctrl-f",
        FocusSearch,
        Some("ClipboardPanel"),
    )]);
}

/// An open context menu inside the panel.
enum Menu {
    Post { index: usize, pos: Point<Pixels> },
    Group { index: usize, pos: Point<Pixels> },
    More { pos: Point<Pixels> },
}

/// An open modal dialog.
enum Modal {
    Rename { group: Option<usize>, post: Option<usize> },
    Shortcut { index: usize, buffer: String },
}

pub struct PanelView {
    posts: Vec<Post>,
    groups: Vec<PostGroup>,
    group_index: usize,
    active: usize,
    keyword: String,
    search_open: bool,
    search: Entity<TextInput>,
    rename: Entity<TextInput>,
    scroll: ScrollHandle,
    menu: Option<Menu>,
    modal: Option<Modal>,
    toast: Option<(SharedString, std::time::Instant)>,
    focus: FocusHandle,
}

impl PanelView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let palette = current_palette(cx, window);
        // The search box reports changes back to the panel entity. These
        // callbacks fire from inside a `TextInput` (and sometimes a `PanelView`)
        // update, so the panel update is deferred: updating it re-entrantly
        // would abort the process.
        let panel = cx.entity().downgrade();
        let panel_change = panel.clone();
        let panel_escape = panel.clone();
        let search = cx.new(|cx| {
            TextInput::new("搜索剪贴板内容", InputStyle::from_palette(&palette), cx)
                .on_change(move |text, _window, cx| {
                    let text = text.to_string();
                    let panel = panel_change.clone();
                    cx.defer(move |cx| {
                        let _ = panel.update(cx, |this, cx| {
                            this.keyword = text;
                            this.reload(cx);
                        });
                    });
                })
                .on_escape(move |window, cx| {
                    let panel = panel_escape.clone();
                    window.defer(cx, move |window: &mut Window, cx: &mut App| {
                        let _ = panel.update(cx, |this, cx| {
                            this.search_open = false;
                            this.keyword.clear();
                            this.search.update(cx, |input, cx| input.clear(cx));
                            let focus = this.focus.clone();
                            window.focus(&focus);
                            this.reload(cx);
                        });
                    });
                })
        });
        let rename = cx.new(|cx| {
            TextInput::new("新名称", InputStyle::from_palette(&palette), cx)
        });
        let focus = cx.focus_handle();
        window.focus(&focus);

        let mut view = PanelView {
            posts: Vec::new(),
            groups: Vec::new(),
            group_index: 0,
            active: 0,
            keyword: String::new(),
            search_open: false,
            search,
            rename,
            scroll: ScrollHandle::new(),
            menu: None,
            modal: None,
            toast: None,
            focus,
        };
        view.reload(cx);
        view
    }

    // ------------------------------------------------------------- loading

    fn current_group_name(&self) -> String {
        if self.group_index == 0 {
            ALL_GROUP_NAME.to_string()
        } else {
            self.groups
                .get(self.group_index - 1)
                .map(|g| g.group_name.clone())
                .unwrap_or_else(|| ALL_GROUP_NAME.to_string())
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let state = cx.global::<AppState>();
        self.groups = state.groups();
        if self.group_index > self.groups.len() {
            self.group_index = 0;
        }
        let group = self.current_group_name();
        self.posts = state.posts_in_group(
            if self.group_index == 0 { "" } else { &group },
            &self.keyword,
            1,
        );
        // A larger window keeps the panel snappy without paging.
        if self.posts.len() < 200 {
            let mut q = PostQuery {
                status: Some(0),
                limit: Some(200),
                ..Default::default()
            };
            if self.group_index != 0 {
                q.group_name = Some(group.clone());
            }
            if !self.keyword.trim().is_empty() {
                q.title = Some(self.keyword.trim().to_string());
            }
            self.posts = state.query_posts(&q);
        }
        if self.active >= self.posts.len() {
            self.active = self.posts.len().saturating_sub(1);
        }
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    // --------------------------------------------------------- interactions

    /// Hide the panel and hand focus back to the previous application.
    fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        clipassistant_core::paste::focus_and_paste(None, false);
        windows::forget_panel(cx);
        window.remove_window();
    }

    fn move_active(&mut self, delta: i64, cx: &mut Context<Self>) {
        if self.posts.is_empty() {
            return;
        }
        let next = self.active as i64 + delta;
        self.active = next.clamp(0, self.posts.len() as i64 - 1) as usize;
        self.scroll.scroll_to_item(self.active);
        cx.notify();
    }

    fn set_group(&mut self, index: usize, cx: &mut Context<Self>) {
        if index <= self.groups.len() {
            self.group_index = index;
            self.active = 0;
            self.reload(cx);
        }
    }

    fn next_group(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = self.groups.len() + 1;
        if count == 0 {
            return;
        }
        let index = if forward {
            (self.group_index + 1) % count
        } else {
            (self.group_index + count - 1) % count
        };
        self.set_group(index, cx);
    }

    /// Entry point used by the previous/next group global shortcuts.
    pub fn move_group_from_hotkey(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.next_group(forward, cx);
    }

    /// Paste the entry at `index` (or the active one).
    fn paste(&mut self, index: usize, as_plain: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(post) = self.posts.get(index).cloned() else {
            return;
        };
        let state = cx.global::<AppState>();
        state.paste_post(&post, as_plain);
        // Reordering puts the pasted entry first next time.
        let id = post.id;
        self.hide(window, cx);
        let _ = id;
    }

    fn remove_index(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(post) = self.posts.get(index).cloned() else {
            return;
        };
        cx.global::<AppState>().remove_post(post.id);
        self.reload(cx);
    }

    fn show_toast(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some((text.into(), std::time::Instant::now()));
        cx.notify();
    }

    fn visible_toast(&self) -> Option<SharedString> {
        self.toast.as_ref().and_then(|(text, at)| {
            (at.elapsed() < std::time::Duration::from_millis(2400)).then(|| text.clone())
        })
    }

    // ------------------------------------------------------------- keyboard

    /// `Ctrl+F` — jump straight into the search box.
    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.open_search(window, cx);
    }

    /// Expand the search box and put the caret in it. An existing keyword is
    /// selected so the first keystroke replaces it.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = true;
        let has_text = !self.search.read(cx).content().is_empty();
        self.search.update(cx, |input, cx| {
            if has_text {
                input.select_all_text(cx);
            }
        });
        let handle = self.search.read(cx).focus_handle(cx);
        window.focus(&handle);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() {
            self.on_modal_key(event, cx);
            return;
        }
        if self.search.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => {
                if self.menu.is_some() {
                    self.menu = None;
                    cx.notify();
                } else if self.search_open {
                    self.close_search(cx);
                } else {
                    self.hide(window, cx);
                }
            }
            "enter" => {
                if self.menu.is_some() {
                    self.menu = None;
                    cx.notify();
                } else {
                    self.paste(self.active, false, window, cx);
                }
            }
            "delete" => {
                self.remove_index(self.active, cx);
            }
            "left" | "up" => self.move_active(-1, cx),
            "right" | "down" => self.move_active(1, cx),
            _ => {
                // Type-to-search: a bare printable character opens the box.
                if event.keystroke.modifiers == Modifiers::default() {
                    if let Some(ch) = event.keystroke.key_char.as_ref() {
                        if ch.chars().count() == 1 && !ch.chars().next().unwrap().is_control() {
                            self.search_open = true;
                            let ch = ch.clone();
                            let handle = self.search.read(cx).focus_handle(cx);
                            window.focus(&handle);
                            self.search.update(cx, |input, cx| {
                                input.insert_str(&ch, window, cx);
                            });
                            // `on_change` on the input refreshes `keyword` + list.
                        }
                    }
                }
            }
        }
    }

    fn on_modal_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.as_mut() else {
            return;
        };
        match modal {
            Modal::Rename { .. } => {
                if event.keystroke.key == "escape" {
                    self.modal = None;
                }
            }
            Modal::Shortcut { buffer, .. } => {
                if event.keystroke.key == "escape" {
                    self.modal = None;
                    cx.notify();
                    return;
                }
                let formatted = format_shortcut(&event.keystroke);
                if let Some(text) = formatted {
                    *buffer = text;
                }
                cx.notify();
            }
        }
    }

    fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search_open = false;
        self.search.update(cx, |input, cx| input.clear(cx));
        self.keyword.clear();
        self.reload(cx);
    }

    // -------------------------------------------------------------- actions

    fn add_group(&mut self, cx: &mut Context<Self>) {
        cx.global::<AppState>().create_group();
        self.reload(cx);
        self.group_index = self.groups.len();
        self.reload(cx);
    }

    fn commit_rename(&mut self, group: Option<usize>, post: Option<usize>, cx: &mut Context<Self>) {
        let name = self.rename.read(cx).content().trim().to_string();
        if name.is_empty() {
            self.modal = None;
            cx.notify();
            return;
        }
        let state = cx.global::<AppState>();
        if let Some(index) = group {
            if let Some(g) = self.groups.get(index) {
                let id = g.id;
                let old = g.group_name.clone();
                state.rename_group(id, &name);
                // Keep posts pointing at the new name.
                for post in state.query_posts(&PostQuery {
                    group_name: Some(old),
                    ..Default::default()
                }) {
                    state.update_post(
                        post.id,
                        &PostPatch {
                            group_name: Some(name.clone()),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        if let Some(index) = post {
            if let Some(p) = self.posts.get(index) {
                let id = p.id;
                state.update_post(
                    id,
                    &PostPatch {
                        title: Some(name.clone()),
                        ..Default::default()
                    },
                );
            }
        }
        self.modal = None;
        self.reload(cx);
    }

    fn commit_shortcut(&mut self, index: usize, buffer: &str, cx: &mut Context<Self>) {
        let Some(post) = self.posts.get(index).cloned() else {
            return;
        };
        let state = cx.global::<AppState>();
        let buffer = buffer.to_string();
        state.store.set_hot_key(
            &post.id.to_string(),
            AppointShortcut {
                id: post.id,
                content_type: post.content_type,
                shortcut: buffer.clone(),
                name: None,
            },
        );
        state.reload_appoint();
        if let Some(hk) = state.hotkeys.lock().unwrap().clone() {
            hk.register(clipassistant_core::hotkey::ids::APPOINT_BASE + post.id as i32 % 1000, &buffer);
        }
        self.modal = None;
        self.show_toast("快捷键已保存", cx);
    }

    // ---------------------------------------------------------------- views

    fn header(&mut self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let search_focused = self.search_open;
        let search_box = div()
            .id("search-box")
            .flex()
            .flex_row()
            .items_center()
            .flex_none()
            .gap(px(4.))
            .rounded(px(6.))
            .px(px(6.))
            .h(px(28.))
            .when(search_focused, |d| {
                d.w(px(190.))
                    .border_1()
                    .border_color(palette.primary)
                    .bg(palette.input)
            })
            .when(!search_focused, |d| d.bg(palette.input))
            .cursor_pointer()
            .child(
                div()
                    .flex_none()
                    .text_size(px(13.))
                    .text_color(if search_focused {
                        palette.primary
                    } else {
                        palette.muted
                    })
                    .child("🔍"),
            )
            .when(search_focused, |d| d.child(div().flex_1().h_full().child(self.search.clone())))
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                this.open_search(window, cx);
            }));

        // Group chips.
        let mut strip = div()
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .min_w(px(0.))
            .gap(px(4.))
            .overflow_hidden();
        strip = strip.child(self.group_chip(0, ALL_GROUP_NAME.to_string(), palette, cx));
        for i in 0..self.groups.len() {
            let name = self.groups[i].group_name.clone();
            strip = strip.child(self.group_chip(i + 1, name, palette, cx));
        }
        strip = strip.child(
            div()
                .id("add-group")
                .flex_none()
                .px(px(6.))
                .py(px(2.))
                .rounded(px(6.))
                .text_size(px(14.))
                .text_color(palette.muted)
                .cursor_pointer()
                .hover(move |s| s.bg(palette.overlay))
                .child("＋")
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                    this.add_group(cx);
                })),
        );

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .flex_none()
            .w_full()
            .h(px(50.))
            .px(px(10.))
            .child(search_box)
            .child(strip)
            .child(
                div()
                    .id("more")
                    .flex_none()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .text_size(px(14.))
                    .text_color(palette.muted)
                    .cursor_pointer()
                    .hover(move |s| s.bg(palette.overlay))
                    .child("⋯")
                    .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                        this.menu = Some(Menu::More {
                            pos: Point {
                                x: px(0.),
                                y: px(0.),
                            },
                        });
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn group_chip(
        &self,
        index: usize,
        name: String,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.group_index == index;
        let color = group_color(&name);
        let is_all = index == 0;
        let id = SharedString::from(format!("group-{index}"));
        div()
            .id(id)
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.))
            .px(px(8.))
            .py(px(3.))
            .rounded(px(6.))
            .max_w(px(120.))
            .when(selected, |d| d.bg(palette.primary_hsla().opacity(0.16)))
            .when(!selected, |d| d.hover(move |s| s.bg(palette.overlay)))
            .cursor_pointer()
            .child(widgets::dot(color))
            .child(
                div()
                    .text_size(px(12.))
                    .truncate()
                    .text_color(if selected {
                        palette.primary
                    } else {
                        palette.font
                    })
                    .child(name),
            )
            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                this.set_group(index, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.menu = Some(if is_all {
                        Menu::Group { index: 0, pos: event.position }
                    } else {
                        Menu::Group { index: index - 1, pos: event.position }
                    });
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn list(&mut self, palette: &Palette, layout: Layout, cx: &mut Context<Self>) -> AnyElement {
        if self.posts.is_empty() {
            return div()
                .flex()
                .items_center()
                .justify_center()
                .flex_1()
                .text_size(px(14.))
                .text_color(palette.muted)
                .child("当前分组下的剪贴板内容为空")
                .into_any_element();
        }

        let handlers = Rc::new(CardHandlers {
            on_click: {
                let entity = cx.entity().downgrade();
                Box::new(move |index, double, window, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        this.active = index;
                        if double {
                            this.paste(index, false, window, cx);
                        } else {
                            cx.notify();
                        }
                    });
                })
            },
            on_context: {
                let entity = cx.entity().downgrade();
                Box::new(move |index, event, _window, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        this.active = index;
                        this.menu = Some(Menu::Post {
                            index,
                            pos: event.position,
                        });
                        cx.notify();
                    });
                })
            },
        });

        let cards: Vec<AnyElement> = self
            .posts
            .iter()
            .enumerate()
            .map(|(i, post)| {
                render_card(post.clone(), i, self.active == i, palette, handlers.clone())
            })
            .collect();

        let horizontal = layout == Layout::Horizontal;
        let mut container = div()
            .id("card-list")
            .flex()
            .flex_1()
            .min_h(px(0.))
            .gap(px(10.))
            .p(px(10.))
            .track_scroll(&self.scroll);
        container = if horizontal {
            container.flex_row().overflow_x_scroll()
        } else {
            container.flex_col().overflow_y_scroll()
        };
        container.children(cards).into_any_element()
    }

    fn menu_overlay(&mut self, palette: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        let (pos, items) = match menu {
            Menu::Post { index, pos } => {
                let index = *index;
                let post = self.posts.get(index).cloned()?;
                let ct = post.content_type();
                let mut items: Vec<(SharedString, AnyElement)> = Vec::new();
                if ct == ContentType::Text {
                    items.push((
                        "粘贴为纯文本".into(),
                        self.menu_action(
                            format!("plain-{index}"),
                            "粘贴为纯文本",
                            palette,
                            move |this, window, cx| this.paste(index, true, window, cx),
                            cx,
                        ),
                    ));
                }
                items.push((
                    "指定快捷键".into(),
                    self.menu_action(
                        format!("shortcut-{index}"),
                        "指定快捷键",
                        palette,
                        move |this, _window, cx| {
                            let current = this
                                .posts
                                .get(index)
                                .map(|p| {
                                    cx.global::<AppState>()
                                        .appoint
                                        .lock()
                                        .unwrap()
                                        .get(&p.id.to_string())
                                        .map(|s| s.shortcut.clone())
                                        .unwrap_or_default()
                                })
                                .unwrap_or_default();
                            this.modal = Some(Modal::Shortcut {
                                index,
                                buffer: current,
                            });
                            cx.notify();
                        },
                        cx,
                    ),
                ));
                if ct == ContentType::Link {
                    items.push((
                        "在浏览器打开".into(),
                        self.menu_action(
                            format!("open-{index}"),
                            "在浏览器打开",
                            palette,
                            move |this, _window, _cx| {
                                if let Some(p) = this.posts.get(index) {
                                    clipassistant_core::winutil::open_with_shell(&p.content);
                                }
                            },
                            cx,
                        ),
                    ));
                }
                items.push((
                    "重命名".into(),
                    self.menu_action(
                        format!("rename-{index}"),
                        "重命名",
                        palette,
                        move |this, _window, cx| {
                            let title = this
                                .posts
                                .get(index)
                                .map(|p| p.title.clone())
                                .unwrap_or_default();
                            this.rename.update(cx, |i, cx| i.set_content(&title, cx));
                            this.modal = Some(Modal::Rename {
                                group: None,
                                post: Some(index),
                            });
                            cx.notify();
                        },
                        cx,
                    ),
                ));
                // "移动到" is rendered as a directly nested list.
                for (gi, group) in self.groups.clone().into_iter().enumerate() {
                    let group_id = group.id;
                    let group_name = group.group_name.clone();
                    items.push((
                        format!("移动:{}", group.group_name).into(),
                        self.menu_action(
                            format!("move-{index}-{gi}"),
                            format!("  移动到 {}", group.group_name),
                            palette,
                            move |this, _window, cx| {
                                let id = this.posts.get(index).map(|p| p.id);
                                if let Some(id) = id {
                                    cx.global::<AppState>().move_post(
                                        id,
                                        Some(group_id),
                                        &group_name,
                                    );
                                }
                                this.menu = None;
                                this.reload(cx);
                            },
                            cx,
                        ),
                    ));
                }
                items.push((
                    "删除".into(),
                    self.menu_action(
                        format!("remove-{index}"),
                        "删除",
                        palette,
                        move |this, _window, cx| {
                            this.menu = None;
                            this.remove_index(index, cx);
                        },
                        cx,
                    ),
                ));
                (pos, items)
            }
            Menu::Group { index, pos } => {
                let index = *index;
                let mut items: Vec<(SharedString, AnyElement)> = Vec::new();
                if index == 0 {
                    items.push((
                        "清空未分组项".into(),
                        self.menu_action(
                            "clear-ungrouped".to_string(),
                            "清空未分组项",
                            palette,
                            |this, _window, cx| {
                                for post in this
                                    .posts
                                    .iter()
                                    .filter(|p| p.group_id.is_none())
                                    .cloned()
                                    .collect::<Vec<_>>()
                                {
                                    cx.global::<AppState>().remove_post(post.id);
                                }
                                this.menu = None;
                                this.reload(cx);
                            },
                            cx,
                        ),
                    ));
                } else {
                    let group_id = self.groups.get(index).map(|g| g.id).unwrap_or(0);
                    let name = self
                        .groups
                        .get(index)
                        .map(|g| g.group_name.clone())
                        .unwrap_or_default();
                    items.push((
                        "重命名".into(),
                        self.menu_action(
                            format!("g-rename-{index}"),
                            "重命名",
                            palette,
                            move |this, _window, cx| {
                                this.rename.update(cx, |i, cx| i.set_content(&name, cx));
                                this.modal = Some(Modal::Rename {
                                    group: Some(index),
                                    post: None,
                                });
                                cx.notify();
                            },
                            cx,
                        ),
                    ));
                    items.push((
                        "左移".into(),
                        self.menu_action(
                            format!("g-left-{index}"),
                            "左移",
                            palette,
                            move |this, _window, cx| {
                                let mut ids: Vec<i64> =
                                    this.groups.iter().map(|g| g.id).collect();
                                if index > 0 && index < ids.len() {
                                    ids.swap(index - 1, index);
                                    cx.global::<AppState>().reorder_groups(&ids);
                                }
                                this.menu = None;
                                this.reload(cx);
                            },
                            cx,
                        ),
                    ));
                    items.push((
                        "右移".into(),
                        self.menu_action(
                            format!("g-right-{index}"),
                            "右移",
                            palette,
                            move |this, _window, cx| {
                                let mut ids: Vec<i64> =
                                    this.groups.iter().map(|g| g.id).collect();
                                if index + 1 < ids.len() {
                                    ids.swap(index, index + 1);
                                    cx.global::<AppState>().reorder_groups(&ids);
                                }
                                this.menu = None;
                                this.reload(cx);
                            },
                            cx,
                        ),
                    ));
                    items.push((
                        "清空此分组".into(),
                        self.menu_action(
                            format!("g-clear-{index}"),
                            "清空此分组",
                            palette,
                            move |this, _window, cx| {
                                cx.global::<AppState>().clear_group(group_id);
                                this.menu = None;
                                this.reload(cx);
                            },
                            cx,
                        ),
                    ));
                    items.push((
                        "删除".into(),
                        self.menu_action(
                            format!("g-remove-{index}"),
                            "删除",
                            palette,
                            move |this, _window, cx| {
                                cx.global::<AppState>().remove_group(group_id);
                                this.group_index = 0;
                                this.menu = None;
                                this.reload(cx);
                            },
                            cx,
                        ),
                    ));
                }
                (pos, items)
            }
            Menu::More { pos } => {
                let items = vec![
                    (
                        "偏好设置".into(),
                        self.menu_action(
                            "m-settings".to_string(),
                            "偏好设置",
                            palette,
                            |this, _window, cx| {
                                this.menu = None;
                                windows::open_settings(cx);
                            },
                            cx,
                        ),
                    ),
                    (
                        "上一个分组".into(),
                        self.menu_action(
                            "m-prev".to_string(),
                            "上一个分组",
                            palette,
                            |this, _window, cx| {
                                this.next_group(false, cx);
                                this.menu = None;
                            },
                            cx,
                        ),
                    ),
                    (
                        "下一个分组".into(),
                        self.menu_action(
                            "m-next".to_string(),
                            "下一个分组",
                            palette,
                            |this, _window, cx| {
                                this.next_group(true, cx);
                                this.menu = None;
                            },
                            cx,
                        ),
                    ),
                    (
                        "使用指南".into(),
                        self.menu_action(
                            "m-guide".to_string(),
                            "使用指南",
                            palette,
                            |_this, _window, _cx| {
                                clipassistant_core::winutil::open_with_shell(
                                    "https://yuyuanweb.yuque.com/org-wiki-yuyuanweb-zvq1bg/dbtzac/syb2hz5gyyqsxtbf",
                                );
                            },
                            cx,
                        ),
                    ),
                    (
                        "退出".into(),
                        self.menu_action(
                            "m-quit".to_string(),
                            "退出",
                            palette,
                            |_this, _window, cx| cx.quit(),
                            cx,
                        ),
                    ),
                ];
                (pos, items)
            }
        };

        let mut card = div()
            .flex()
            .flex_col()
            .min_w(px(150.))
            .p(px(6.))
            .rounded(px(10.))
            .bg(palette.surface)
            .shadow_lg();
        for (_, item) in items {
            card = card.child(item);
        }

        let overlay = div()
            .id("menu-overlay")
            .absolute()
            .inset_0()
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                this.menu = None;
                cx.notify();
            }))
            .child(
                div()
                    .absolute()
                    .left(pos.x.max(px(4.)))
                    .top(pos.y.max(px(4.)))
                    .child(card),
            )
            .into_any_element();
        Some(overlay)
    }

    fn menu_action(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        palette: &Palette,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity().downgrade();
        let label: SharedString = label.into();
        div()
            .id(id.into())
            .px(px(10.))
            .py(px(5.))
            .rounded(px(6.))
            .text_size(px(12.))
            .text_color(palette.font)
            .cursor_pointer()
            .hover(move |s| s.bg(palette.primary_hsla().opacity(0.14)))
            .child(label)
            .on_click(move |_, window, cx| {
                let _ = entity.update(cx, |this, cx| action(this, window, cx));
            })
            .into_any_element()
    }

    fn modal_overlay(&mut self, palette: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        match self.modal.as_ref()? {
            Modal::Rename { group, post } => {
                let group = *group;
                let post = *post;
                Some(
                    div()
                        .id("rename-modal")
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
                                .gap(px(10.))
                                .p(px(16.))
                                .rounded(px(12.))
                                .w(px(260.))
                                .bg(palette.surface)
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(palette.font)
                                        .child(if group.is_some() { "重命名分组" } else { "重命名内容" }),
                                )
                                .child(
                                    div()
                                        .h(px(30.))
                                        .px(px(6.))
                                        .rounded(px(6.))
                                        .bg(palette.input)
                                        .child(self.rename.clone()),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_end()
                                        .gap(px(8.))
                                        .child(widgets::ghost_button(
                                            "rename-cancel",
                                            "取消",
                                            palette,
                                            cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                                                this.modal = None;
                                                cx.notify();
                                            }),
                                        ))
                                        .child(widgets::primary_button(
                                            "rename-ok",
                                            "确认",
                                            palette,
                                            cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                                                this.commit_rename(group, post, cx);
                                            }),
                                        )),
                                ),
                        )
                        .into_any_element(),
                )
            }
            Modal::Shortcut { index, buffer } => {
                let index = *index;
                let buffer = buffer.clone();
                Some(
                    div()
                        .id("shortcut-modal")
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
                                .gap(px(10.))
                                .p(px(16.))
                                .rounded(px(12.))
                                .w(px(260.))
                                .bg(palette.surface)
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(palette.font)
                                        .child("为内容指定快捷键"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .h(px(34.))
                                        .rounded(px(6.))
                                        .bg(palette.input)
                                        .text_size(px(12.))
                                        .text_color(palette.font)
                                        .child(if buffer.is_empty() {
                                            SharedString::from("请按下按键组合")
                                        } else {
                                            SharedString::from(buffer.clone())
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(palette.muted)
                                        .child("按 Esc 取消，按回车保存"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_end()
                                        .gap(px(8.))
                                        .child(widgets::primary_button(
                                            "shortcut-ok",
                                            "保存",
                                            palette,
                                            cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                                                let buffer = match this.modal.as_ref() {
                                                    Some(Modal::Shortcut { buffer, .. }) => buffer.clone(),
                                                    _ => String::new(),
                                                };
                                                this.commit_shortcut(index, &buffer, cx);
                                            }),
                                        )),
                                ),
                        )
                        .into_any_element(),
                )
            }
        }
    }
}

impl Render for PanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = current_palette(cx, window);
        let layout = cx.global::<AppState>().settings().clipboard.layout;

        let mut root = div()
            .id("clipboard-panel")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .rounded(px(12.))
            .bg(palette.backdrop)
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("ClipboardPanel")
            .on_action(cx.listener(Self::focus_search))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(self.header(&palette, cx))
            .child(self.list(&palette, layout, cx));

        if let Some(menu) = self.menu_overlay(&palette, cx) {
            root = root.child(menu);
        }
        if let Some(modal) = self.modal_overlay(&palette, cx) {
            root = root.child(modal);
        }
        if let Some(toast) = self.visible_toast() {
            root = root.child(
                div()
                    .absolute()
                    .left(px(0.))
                    .right(px(0.))
                    .top(px(56.))
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
                            .child(toast),
                    ),
            );
        }
        root
    }
}

/// Resolve the palette from settings + the window's current appearance.
pub fn current_palette(cx: &App, window: &Window) -> Palette {
    let setting = cx.global::<AppState>().settings().clipboard.theme;
    let system_dark = matches!(
        window.appearance(),
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
    );
    Palette::resolve(setting, system_dark)
}

/// Format a keystroke the way the original does (`ctrl+shift+a`).
pub fn format_shortcut(keystroke: &gpui::Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    if matches!(key, "control" | "alt" | "shift" | "meta" | "super" | "windows") {
        return None;
    }
    let m = keystroke.modifiers;
    let mut out = String::new();
    if m.control {
        out.push_str("ctrl+");
    }
    if m.alt {
        out.push_str("alt+");
    }
    if m.shift {
        out.push_str("shift+");
    }
    if m.platform {
        out.push_str("command+");
    }
    let name = match key {
        "arrowleft" => "left",
        "arrowright" => "right",
        "arrowup" => "up",
        "arrowdown" => "down",
        "escape" => "escape",
        "enter" => "enter",
        "space" => "space",
        "tab" => "tab",
        "backspace" => "backspace",
        "delete" => "delete",
        other => other,
    };
    out.push_str(name);
    Some(out)
}
