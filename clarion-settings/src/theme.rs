//! Visual tokens and text shaping for the native settings window.
//!
//! The palette follows the reference System Settings sheet: a quiet sidebar,
//! a near-white detail pane and grouped cards. Text is shaped with explicit
//! Latin/CJK runs because this GPUI fork does not apply font fallback lists on
//! Linux yet.

use gpui::prelude::*;
use gpui::{
    font, px, rgb, rgba, App, FontWeight, Hsla, SharedString, StyledText, TextRun,
};

pub const SIDEBAR_WIDTH: f32 = 340.;
pub const HEADER_HEIGHT: f32 = 70.;
pub const SIDEBAR_ITEM: f32 = 44.;
pub const CARD_RADIUS: f32 = 16.;
pub const ROW_HEIGHT: f32 = 50.;
pub const WINDOW_RADIUS: f32 = 26.;
pub const PANEL_RADIUS: f32 = 16.;
pub const GUTTER: f32 = 10.;
/// Global type step. The reference sheet is generously sized; every label in
/// this app goes through `widgets::label`, so one multiplier keeps text,
/// spacing and icon rows in proportion.
pub const TEXT_SCALE: f32 = 1.35;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Palette {
    pub dark: bool,
    pub window: Hsla,
    pub window_alt: Hsla,
    pub window_border: Hsla,
    pub sidebar: Hsla,
    pub sidebar_alt: Hsla,
    pub detail: Hsla,
    pub detail_alt: Hsla,
    pub chrome: Hsla,
    pub chrome_alt: Hsla,
    pub panel_border: Hsla,
    pub card: Hsla,
    pub card_border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
    pub on_accent: Hsla,
    pub separator: Hsla,
    pub hover: Hsla,
    pub control: Hsla,
    pub control_border: Hsla,
    pub switch_off: Hsla,
    pub segment_bg: Hsla,
    pub chip: Hsla,
    pub chip_hover: Hsla,
    pub search_bg: Hsla,
    pub search_border: Hsla,
    pub shadow: Hsla,
}

pub fn palette(dark: bool, accent_hex: &str) -> Palette {
    let accent = accent(accent_hex);
    if dark {
        Palette {
            dark,
            window: rgba(0x20242c9e).into(),
            window_alt: rgba(0x15181e8f).into(),
            window_border: rgba(0xffffff26).into(),
            sidebar: rgba(0x353a44b8).into(),
            sidebar_alt: rgba(0x2a2f37ae).into(),
            detail: rgba(0x1e2127d0).into(),
            detail_alt: rgba(0x1e2127aa).into(),
            chrome: rgba(0x2d323cc8).into(),
            chrome_alt: rgba(0x262b34bc).into(),
            panel_border: rgba(0xffffff26).into(),
            card: rgba(0x3d455340).into(),
            card_border: rgba(0xffffff0d).into(),
            text: rgb(0xf5f5f7).into(),
            muted: rgb(0xa1a6b0).into(),
            accent,
            accent_soft: accent.opacity(0.24),
            on_accent: rgb(0xffffff).into(),
            separator: rgba(0xffffff12).into(),
            hover: rgba(0xffffff0b).into(),
            control: rgba(0x424957b8).into(),
            control_border: rgba(0xffffff1a).into(),
            switch_off: rgba(0x555b66cc).into(),
            segment_bg: rgba(0xffffff0d).into(),
            chip: rgba(0xffffff14).into(),
            chip_hover: rgba(0xffffff26).into(),
            search_bg: rgba(0x0b0d11b0).into(),
            search_border: rgba(0xffffff24).into(),
            shadow: rgba(0x00000066).into(),
        }
    } else {
        Palette {
            dark,
            window: rgba(0xf5f8fcb3).into(),
            window_alt: rgba(0xe9eef6a3).into(),
            window_border: rgba(0x16203326).into(),
            sidebar: rgba(0xffffffc4).into(),
            sidebar_alt: rgba(0xf4f8ffbc).into(),
            detail: rgba(0xfbfcfef0).into(),
            detail_alt: rgba(0xfbfcfed8).into(),
            chrome: rgba(0xf7fafec2).into(),
            chrome_alt: rgba(0xf0f5fcb8).into(),
            panel_border: rgba(0x1a24331c).into(),
            card: rgba(0xe6ebf290).into(),
            card_border: rgba(0x00000010).into(),
            text: rgb(0x1d1d1f).into(),
            muted: rgb(0x666b74).into(),
            accent,
            accent_soft: accent.opacity(0.15),
            on_accent: rgb(0xffffff).into(),
            separator: rgba(0x00000012).into(),
            hover: rgba(0x00000008).into(),
            control: rgba(0xffffffd9).into(),
            control_border: rgba(0x00000018).into(),
            switch_off: rgba(0xd2d6ddcc).into(),
            segment_bg: rgba(0x0000000d).into(),
            chip: rgba(0x00000010).into(),
            chip_hover: rgba(0x00000020).into(),
            search_bg: rgba(0x00000018).into(),
            search_border: rgba(0x0000001f).into(),
            shadow: rgba(0x1f2a3a33).into(),
        }
    }
}

fn accent(hex: &str) -> Hsla {
    let value = hex
        .strip_prefix('#')
        .filter(|value| value.len() == 6)
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .unwrap_or(0x0a84ff);
    rgb(value).into()
}

/// Shaped text with the same CJK/Latin split the Shell uses. The returned value
/// can be passed to any `text_size` div.
pub fn text(
    value: impl Into<String>,
    weight: FontWeight,
    color: Hsla,
    cx: &App,
) -> StyledText {
    static CJK: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let value = value.into();
    let cjk = CJK.get_or_init(|| {
        let names = cx.text_system().all_font_names();
        if names.iter().any(|name| name == "PingFang SC") {
            "PingFang SC".to_owned()
        } else {
            "Noto Sans CJK SC".to_owned()
        }
    });
    let latin = latin_font(cx);
    let mut runs: Vec<TextRun> = Vec::new();
    for character in value.chars() {
        let family = if matches!(character as u32,
            0x2e80..=0xa4cf | 0xac00..=0xd7af | 0xf900..=0xfaff |
            0xff00..=0xffef | 0x20000..=0x3134f)
        {
            SharedString::from(cjk.clone())
        } else {
            latin.clone()
        };
        if let Some(last) = runs.last_mut().filter(|run| run.font.family == family) {
            last.len += character.len_utf8();
        } else {
            let mut face = font(family);
            face.weight = weight;
            runs.push(TextRun {
                len: character.len_utf8(),
                font: face,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
    }
    StyledText::new(value).with_runs(runs)
}

fn latin_font(cx: &App) -> SharedString {
    static FAMILY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    FAMILY
        .get_or_init(|| {
            let available = cx.text_system().all_font_names();
            [
                "SF Pro Text",
                ".SF NS Text",
                "System Font",
                ".SF NS",
                "SF Pro",
                "Inter",
                "Noto Sans",
            ]
            .into_iter()
            .find(|wanted| available.iter().any(|name| name == wanted))
            .unwrap_or("Noto Sans CJK SC")
            .to_owned()
        })
        .clone()
        .into()
}

pub fn hairline(p: Palette) -> gpui::Div {
    gpui::div().h(px(1.)).w_full().bg(p.separator)
}
