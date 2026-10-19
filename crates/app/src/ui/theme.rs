//! Colour palette.
//!
//! Values are taken verbatim from the original Tailwind build so the
//! reimplementation matches the shipped application:
//! `light.primary = #726CFF`, `dark.primary = #CD75FF`,
//! `dark.card.background = #222223`, `dark.clipboard.background = #171719`.

use gpui::{rgb, rgba, Hsla, Rgba};
use clipassistant_core::settings::Theme as ThemeSetting;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Accent colour (`light.primary` / `dark.primary`).
    pub primary: Rgba,
    pub primary_fg: Rgba,
    /// Panel backdrop (`bg-gray-200/90`, `dark-clipboard-background`).
    pub backdrop: Rgba,
    /// Card surface (`bg-[#F3F3F3]`, `dark-card-background`).
    pub card: Rgba,
    /// Secondary surface (`dark-card-foreground`).
    pub surface: Rgba,
    /// Input / chip surface (`bg-[#EFEFEF]`, `dark-shortcut`).
    pub input: Rgba,
    pub font: Rgba,
    pub muted: Rgba,
    pub border: Rgba,
    /// Card footer strip colour pulled from the group name hash.
    pub overlay: Rgba,
    /// Backdrop of a code snippet inside a text card.
    pub code_bg: Rgba,
    pub code_font: Rgba,
    pub code_keyword: Rgba,
    pub code_string: Rgba,
    pub code_number: Rgba,
    pub code_comment: Rgba,
}

impl Palette {
    pub fn light() -> Palette {
        Palette {
            dark: false,
            primary: rgb(0x726CFF),
            primary_fg: rgb(0xFFFFFF),
            backdrop: rgba(0xE5E7EBE6),
            card: rgb(0xF3F3F3),
            surface: rgb(0xFFFFFF),
            input: rgb(0xEFEFEF),
            font: rgb(0x4D4D4D),
            muted: rgb(0x9CA3AF),
            border: rgba(0x00000014),
            overlay: rgba(0x00000010),
            code_bg: rgb(0xFFFFFF),
            code_font: rgb(0x31313A),
            code_keyword: rgb(0xB4357A),
            code_string: rgb(0x2F7D32),
            code_number: rgb(0xB45309),
            code_comment: rgb(0x9CA3AF),
        }
    }

    pub fn dark() -> Palette {
        Palette {
            dark: true,
            primary: rgb(0xCD75FF),
            primary_fg: rgb(0xFFFFFF),
            backdrop: rgb(0x171719),
            card: rgb(0x222223),
            surface: rgb(0x3B3B47),
            input: rgb(0x1E1E21),
            font: rgb(0xEEEEEE),
            muted: rgb(0xA2A2A2),
            border: rgba(0xFFFFFF14),
            overlay: rgba(0xFFFFFF10),
            code_bg: rgb(0x1A1A1B),
            code_font: rgb(0xE4E4E7),
            code_keyword: rgb(0xFF6EA9),
            code_string: rgb(0xC3E88D),
            code_number: rgb(0xFF9E64),
            code_comment: rgb(0x7A7A85),
        }
    }

    /// Resolve the palette from the persisted setting.
    pub fn resolve(setting: ThemeSetting, system_dark: bool) -> Palette {
        match setting {
            ThemeSetting::Light => Palette::light(),
            ThemeSetting::Dark => Palette::dark(),
            ThemeSetting::System => {
                if system_dark {
                    Palette::dark()
                } else {
                    Palette::light()
                }
            }
        }
    }

    pub fn primary_hsla(&self) -> Hsla {
        self.primary.into()
    }
}

/// Stable colour for a group name — the original hashes the name and takes the
/// low 24 bits (`utils` → `colorFromName`).
pub fn group_color(name: &str) -> Hsla {
    let mut hash: i32 = 0;
    for ch in name.chars() {
        hash = hash.wrapping_shl(5).wrapping_sub(hash).wrapping_add(ch as i32);
    }
    let r = ((hash & 0x00FF0000) >> 16) as u8;
    let g = ((hash & 0x0000FF00) >> 8) as u8;
    let b = (hash & 0x000000FF) as u8;
    rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0x55).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_follows_system() {
        assert!(Palette::resolve(ThemeSetting::System, true).dark);
        assert!(!Palette::resolve(ThemeSetting::System, false).dark);
        assert!(Palette::resolve(ThemeSetting::Dark, false).dark);
    }

    #[test]
    fn group_colour_is_stable() {
        assert_eq!(group_color("工作"), group_color("工作"));
        assert_ne!(group_color("工作"), group_color("生活"));
    }
}
