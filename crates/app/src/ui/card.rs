//! Clipboard entry cards.
//!
//! One renderer per content type, mirroring the four components of the
//! original renderer bundle (text / image / file / link), each 250×250 with a
//! coloured footer carrying the source app and timestamp.

use super::highlight;
use super::theme::{group_color, Palette};
use super::widgets;
use gpui::{
    div, prelude::*, px, AnyElement, Hsla, Image, ImageFormat, MouseButton, SharedString,
};
use clipassistant_core::model::{ContentType, FileDescriptor, Post};
use clipassistant_core::util;
use std::rc::Rc;
use std::sync::Arc;

pub const CARD_SIZE: f32 = 250.0;

/// Height of the coloured title strip on text cards.
const HEADER_H: f32 = 34.0;
/// How many lines of a snippet fit under the strip.
const CODE_LINES: usize = 9;
const CODE_LINE_H: f32 = 17.0;
/// Roughly what fits across the card at 11px monospace.
const CODE_COLUMNS: usize = 36;
const CODE_FONT: &str = if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "Menlo"
};

/// Callbacks a card needs from its list.
pub struct CardHandlers {
    pub on_click: Box<dyn Fn(usize, bool, &mut gpui::Window, &mut gpui::App)>,
    pub on_context: Box<dyn Fn(usize, &gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App)>,
}

fn source_app(post: &Post) -> clipassistant_core::source_app::SourceApp {
    post.source_app
        .as_deref()
        .map(clipassistant_core::source_app::SourceApp::from_json)
        .unwrap_or_default()
}

/// Render one card.
pub fn render_card(
    post: Post,
    index: usize,
    selected: bool,
    palette: &Palette,
    handlers: Rc<CardHandlers>,
) -> AnyElement {
    let footer_bg = group_color(post.group_name.as_deref().unwrap_or_default());
    let app = source_app(&post);
    let app_name = if app.name.is_empty() {
        "未知应用".to_string()
    } else {
        app.name.clone()
    };
    let time = util::time_ago(&post.edit_time);
    let content_type = post.content_type();

    let body = match content_type {
        ContentType::Text => text_body(&post, palette),
        ContentType::Link => link_body(&post, palette),
        ContentType::Image => image_body(&post, palette),
        ContentType::File | ContentType::Folder => file_body(&post, palette),
    };

    let click_handlers = handlers.clone();
    let ctx_handlers = handlers.clone();
    let id = SharedString::from(format!("card-{}", post.id));

    let card = div()
        .id(id)
        .flex_none()
        .relative()
        .w(px(CARD_SIZE))
        .h(px(CARD_SIZE))
        .rounded(px(12.))
        .bg(palette.card)
        .overflow_hidden()
        .flex()
        .flex_col()
        .cursor_pointer()
        .when(selected, |d| {
            d.border_2().border_color(palette.primary)
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.))
                .overflow_hidden()
                .child(body),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .h(px(46.))
                .flex_none()
                .px(px(8.))
                .bg(footer_bg)
                .child(
                    div()
                        .size(px(30.))
                        .rounded(px(6.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(palette.surface)
                        .text_size(px(13.))
                        .text_color(palette.primary)
                        .child(initial_of(&app_name)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w(px(0.))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(palette.font)
                                .truncate()
                                .child(app_name),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(palette.muted)
                                .child(time),
                        ),
                ),
        )
        .on_click(move |event, window, cx| {
            let double = event.click_count() >= 2;
            (click_handlers.on_click)(index, double, window, cx);
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            (ctx_handlers.on_context)(index, event, window, cx);
        });

    card.into_any_element()
}

fn initial_of(name: &str) -> SharedString {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
        .into()
}

// ------------------------------------------------------------------ bodies

fn title_row(post: &Post, right: Option<SharedString>, palette: &Palette) -> AnyElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(6.))
        .w_full()
        .flex_none()
        .px(px(10.))
        .py(px(8.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(palette.font)
                .truncate()
                .flex_1()
                .min_w(px(0.))
                .child(title_of(post)),
        );
    if let Some(right) = right {
        row = row.child(
            div()
                .flex_none()
                .text_size(px(10.))
                .text_color(palette.muted)
                .child(right),
        );
    }
    row.into_any_element()
}

fn text_body(post: &Post, palette: &Palette) -> AnyElement {
    let plain = clipassistant_core::util::html_to_text(&post.content);
    let chars = plain.chars().count();
    let body = if highlight::looks_like_code(&plain) {
        code_snippet(&plain, palette)
    } else {
        prose(&plain, palette)
    };
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(text_header(post, chars, palette))
        .child(body)
        .into_any_element()
}

fn title_of(post: &Post) -> String {
    if post.title.is_empty() {
        post.content_type().default_title().to_string()
    } else {
        post.title.clone()
    }
}

/// Coloured strip carrying the title and the character count.
fn text_header(post: &Post, chars: usize, palette: &Palette) -> AnyElement {
    let dim: Hsla = palette.primary_fg.into();
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(6.))
        .w_full()
        .h(px(HEADER_H))
        .flex_none()
        .px(px(10.))
        .bg(palette.primary)
        .child(
            div()
                .text_size(px(12.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(palette.primary_fg)
                .truncate()
                .flex_1()
                .min_w(px(0.))
                .child(title_of(post)),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(10.))
                .text_color(dim.opacity(0.75))
                .child(format!("{chars} 字符")),
        )
        .into_any_element()
}

/// Monospace, syntax-coloured snippet with one row per source line.
fn code_snippet(plain: &str, palette: &Palette) -> AnyElement {
    let mut body = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .overflow_hidden()
        .w_full()
        .bg(palette.code_bg)
        .px(px(10.))
        .py(px(8.));
    for tokens in highlight::snippet(plain, CODE_LINES, CODE_COLUMNS) {
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(CODE_LINE_H))
            .flex_none()
            .overflow_hidden();
        for token in tokens {
            row = row.child(
                div()
                    .text_size(px(11.))
                    .line_height(px(CODE_LINE_H))
                    .font_family(CODE_FONT)
                    .text_color(token_color(token.kind, palette))
                    .child(SharedString::from(token.text)),
            );
        }
        body = body.child(row);
    }
    body.into_any_element()
}

fn prose(plain: &str, palette: &Palette) -> AnyElement {
    div()
        .flex_1()
        .min_h(px(0.))
        .w_full()
        .overflow_hidden()
        .px(px(10.))
        .py(px(8.))
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(palette.font)
        .child(SharedString::from(
            plain.chars().take(260).collect::<String>(),
        ))
        .into_any_element()
}

fn token_color(kind: highlight::Kind, palette: &Palette) -> Hsla {
    let colour = match kind {
        highlight::Kind::Keyword => palette.code_keyword,
        highlight::Kind::Str => palette.code_string,
        highlight::Kind::Number => palette.code_number,
        highlight::Kind::Comment => palette.code_comment,
        highlight::Kind::Plain => palette.code_font,
    };
    colour.into()
}

fn link_body(post: &Post, palette: &Palette) -> AnyElement {
    let url = post.content.clone();
    let host = url
        .split("//")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .unwrap_or(url.as_str())
        .to_string();
    div()
        .flex()
        .flex_col()
        .items_center()
        .size_full()
        .child(title_row(post, None, palette))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .flex_1()
                .gap(px(8.))
                .px(px(12.))
                .child(
                    div()
                        .size(px(48.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(palette.primary_hsla().opacity(0.15))
                        .text_size(px(20.))
                        .text_color(palette.primary)
                        .child("🔗"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(palette.font)
                        .truncate()
                        .max_w_full()
                        .child(host),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(palette.muted)
                        .truncate()
                        .max_w_full()
                        .child(post.content.clone()),
                ),
        )
        .into_any_element()
}

fn image_body(post: &Post, palette: &Palette) -> AnyElement {
    let image = if post.content.starts_with("data:") {
        util::decode_data_url(&post.content)
            .map(|(_, bytes)| Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
    } else if let Ok(fd) = serde_json::from_str::<FileDescriptor>(&post.content) {
        some_path_image(&fd.content)
    } else {
        None
    };

    let inner: AnyElement = match image {
        Some(image) => gpui::img(image).into_any_element(),
        None => widgets::image_placeholder("图片不可用", palette).into_any_element(),
    };

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(title_row(post, None, palette))
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .items_center()
                .justify_center()
                .p(px(8.))
                .child(div().size_full().child(inner)),
        )
        .into_any_element()
}

fn some_path_image(path: &str) -> Option<Arc<Image>> {
    let bytes = std::fs::read(path).ok()?;
    let format = match util::extension(path).as_str() {
        ".jpg" | ".jpeg" => ImageFormat::Jpeg,
        ".gif" => ImageFormat::Gif,
        ".webp" => ImageFormat::Webp,
        ".bmp" => ImageFormat::Bmp,
        _ => ImageFormat::Png,
    };
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn file_body(post: &Post, palette: &Palette) -> AnyElement {
    let fd: FileDescriptor = serde_json::from_str(&post.content).unwrap_or(FileDescriptor {
        content: post.content.clone(),
        size: None,
        title: post.title.clone(),
        kind: 1,
        url: None,
    });
    let (glyph, label) = match fd.kind {
        0 => ("🖼", "图片"),
        2 => ("📁", "文件夹"),
        _ => ("📄", "文件"),
    };
    let size = fd.size.map(util::format_size).unwrap_or_default();

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(title_row(post, Some(size.into()), palette))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .flex_1()
                .gap(px(6.))
                .px(px(12.))
                .child(
                    div()
                        .size(px(52.))
                        .rounded(px(10.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(palette.primary_hsla().opacity(0.12))
                        .text_size(px(24.))
                        .child(glyph),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(palette.font)
                        .truncate()
                        .max_w_full()
                        .child(if fd.title.is_empty() {
                            util::file_name(&fd.content)
                        } else {
                            fd.title.clone()
                        }),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(palette.muted)
                        .truncate()
                        .max_w_full()
                        .child(label),
                ),
        )
        .into_any_element()
}

/// The small `<img>`-free element helper kept for future media types.
pub fn _img(source: Arc<Image>) -> AnyElement {
    gpui::img(source).into_any_element()
}
