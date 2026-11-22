//! Pane artwork. The bundled SVGs are rasterized once per process and handed
//! to GPUI as image sources, so colour artwork keeps its own palette (unlike
//! `svg`, which is tinted by the current text colour).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::prelude::*;
use gpui::{img, px, AnyElement, Image, ImageFormat, IntoElement};

/// Rasterize at the physical size the sidebar actually paints. Downscaling a
/// large texture in GPUI's atlases is bilinear and has no mipmaps, so a 256 px
/// source drawn at 30 pt (60 device px) turns soft and aliased at the edges.
fn raster_size(size: f32, scale_factor: f32) -> u32 {
    (size * scale_factor).round().max(1.) as u32
}

pub const OVERVIEW: &str = "general";
pub const APPEARANCE: &str = "appearance";
pub const DOCK: &str = "dock";
pub const MENU_BAR: &str = "menu-bar";
pub const ACCESSIBILITY: &str = "accessibility";
pub const WINDOW_EFFECTS: &str = "animation";
pub const LAYOUT: &str = "window-layout";
pub const MISSION_CONTROL: &str = "mission-control";
pub const DISPLAYS: &str = "display";
pub const POWER: &str = "power";
pub const KEYBOARD: &str = "keyboard";
pub const TRACKPAD: &str = "trackpad";
pub const SYSTEM: &str = "system";

fn svg(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "general" => include_bytes!("../assets/icons/general.svg"),
        "animation" => include_bytes!("../assets/icons/animation.svg"),
        "appearance" => include_bytes!("../assets/icons/appearance.svg"),
        "dock" => include_bytes!("../assets/icons/dock.svg"),
        "menu-bar" => include_bytes!("../assets/icons/menu-bar.svg"),
        "accessibility" => include_bytes!("../assets/icons/accessibility.svg"),
        "window-layout" => include_bytes!("../assets/icons/window-layout.svg"),
        "mission-control" => include_bytes!("../assets/icons/mission-control.svg"),
        "display" => include_bytes!("../assets/icons/display.svg"),
        "power" => include_bytes!("../assets/icons/power.svg"),
        "keyboard" => include_bytes!("../assets/icons/keyboard.svg"),
        "system" => include_bytes!("../assets/icons/system.svg"),
        // The bundled file is named `track.svg`; keep the pane's public name.
        "trackpad" | "track" => include_bytes!("../assets/icons/track.svg"),
        _ => return None,
    })
}

/// Full-colour SVG → PNG at exactly `pixels` device pixels on its longest
/// side, using the renderer GPUI already links.
fn render_svg(bytes: &[u8], pixels: u32) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let source = tree.size();
    let longest = source.width().max(source.height());
    if longest <= 0. || pixels == 0 {
        return None;
    }
    let pixels = pixels as f32;
    let scale = pixels / longest;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pixels as u32, pixels as u32)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(
        (pixels - source.width() * scale) / 2.,
        (pixels - source.height() * scale) / 2.,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

type ImageCache = HashMap<(&'static str, u32), Option<Arc<Image>>>;

pub fn image(name: &'static str, pixels: u32) -> Option<Arc<Image>> {
    static CACHE: OnceLock<Mutex<ImageCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (name, pixels);
    if let Some(cached) = cache.lock().unwrap().get(&key) {
        return cached.clone();
    }
    let image = svg(name)
        .and_then(|bytes| render_svg(bytes, pixels))
        .map(|png| Arc::new(Image::from_bytes(ImageFormat::Png, png)));
    cache.lock().unwrap().insert(key, image.clone());
    image
}

pub fn element(name: &'static str, size: f32, scale_factor: f32) -> AnyElement {
    match image(name, raster_size(size, scale_factor)) {
        Some(image) => img(image).size(px(size)).object_fit(gpui::ObjectFit::Contain).into_any_element(),
        None => gpui::div()
            .size(px(size))
            .rounded(px(size * 0.24))
            .bg(gpui::rgba(0x8e8e93ff))
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pane_svg_rasterizes_to_png() {
        for name in [
            "general",
            "animation",
            "appearance",
            "dock",
            "menu-bar",
            "accessibility",
            "window-layout",
            "mission-control",
            "display",
            "power",
            "keyboard",
            "trackpad",
            "system",
        ] {
            let png = svg(name)
                .and_then(|bytes| render_svg(bytes, 60))
                .unwrap_or_else(|| panic!("{name} is missing or unreadable"));
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "{name} does not rasterize to PNG");
            assert_eq!(&png[16..20], &60u32.to_be_bytes(), "{name} width");
            assert_eq!(&png[20..24], &60u32.to_be_bytes(), "{name} height");
        }
    }
}
