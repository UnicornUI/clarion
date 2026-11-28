//! GPU-rendered, downscaled window previews for the Shell's Dock.

use anyhow::Context;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::utils::{
    Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::{Bind, Offscreen, RendererSuper, Texture};
use smithay::utils::{Physical, Point, Scale, Size, Transform};

use crate::layout::{LayoutElement as _, LayoutElementRenderElement};
use crate::render_helpers::renderer::ClarionRenderer;
use crate::render_helpers::{encompassing_geo, render_to_vec, RenderCtx, RenderTarget};
use crate::window::Mapped;

/// One small preview ready to be handed to the Shell.
pub(crate) struct ThumbnailPixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Render `mapped` into an offscreen buffer whose longest edge is `max`.
///
/// The window is scaled on the GPU, so the readback is only the final small
/// image instead of the native window size. This is the cheap path for the
/// Dock; it deliberately does not go through `ext-image-copy-capture`, whose
/// window constraints stay at native resolution for recording clients.
pub(crate) fn render_window_thumbnail<R>(
    renderer: &mut R,
    mapped: &Mapped,
    output_scale: f64,
    max: u32,
) -> anyhow::Result<Option<ThumbnailPixels>>
where
    R: ClarionRenderer
        + Offscreen<<R as RendererSuper>::TextureId>
        + Bind<<R as RendererSuper>::TextureId>,
    LayoutElementRenderElement<R>: RenderElement<R>,
    for<'a> R::Framebuffer<'a>: Texture,
{
    let scale = Scale::from(output_scale);
    let mut native = Vec::new();
    {
        let mut ctx = RenderCtx {
            renderer,
            target: RenderTarget::ScreenCapture,
            xray: None,
        };
        mapped.render_normal(
            ctx.r(),
            Point::from((0., 0.)),
            scale,
            1.,
            &mut |element| native.push(element),
        );
    }
    if native.is_empty() {
        return Ok(None);
    }
    let geo = encompassing_geo(scale, native.iter());
    if geo.size.w <= 0 || geo.size.h <= 0 {
        return Ok(None);
    }

    let max = max.clamp(16, 2048);
    let (target_w, target_h) = fit(
        geo.size.w as u32,
        geo.size.h as u32,
        max,
    );
    if target_w == 0 || target_h == 0 {
        return Ok(None);
    }
    let factor = target_w as f64 / geo.size.w as f64;
    let elements: Vec<_> = native
        .into_iter()
        .map(|element| {
            let relocated = RelocateRenderElement::from_element(
                element,
                geo.loc.upscale(-1),
                Relocate::Relative,
            );
            RescaleRenderElement::from_element(
                relocated,
                Point::<i32, Physical>::from((0, 0)),
                Scale::from(factor),
            )
        })
        .collect();

    let target: Size<i32, Physical> = Size::from((target_w as i32, target_h as i32));
    let mut bgra = render_to_vec(
        renderer,
        target,
        scale,
        Transform::Normal,
        Fourcc::Abgr8888,
        elements.into_iter(),
    )
    .context("error rendering the window thumbnail")?;
    // `Abgr8888` readback lands as RGBA bytes (the screenshot encoder writes
    // them straight to PNG). GPUI's `RenderImage` stores BGRA, so swap the
    // colour channels on the small target instead of in the Shell.
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }

    Ok(Some(ThumbnailPixels {
        width: target_w,
        height: target_h,
        bgra,
    }))
}

/// Preserve the source aspect ratio and never enlarge.
pub(crate) fn fit(width: u32, height: u32, max: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max || longest == 0 {
        (width, height)
    } else {
        let width = ((width as u64 * max as u64) / longest as u64).max(1) as u32;
        let height = ((height as u64 * max as u64) / longest as u64).max(1) as u32;
        (width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn fit_preserves_aspect_and_never_enlarges() {
        assert_eq!(fit(1920, 1080, 256), (256, 144));
        assert_eq!(fit(800, 1200, 256), (170, 256));
        assert_eq!(fit(200, 150, 256), (200, 150));
        assert_eq!(fit(0, 0, 256), (0, 0));
    }
}
