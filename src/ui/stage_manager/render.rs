//! Offscreen card textures and the mesh render path for the Stage Manager
//! rail.

use smithay::backend::renderer::element::utils::{
    Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::backend::renderer::element::{Element as _, Id, Kind};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::CommitCounter;
use clarion_config::CornerRadius;
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use crate::clarion_render_elements;
use crate::layout::scene_source::WindowLookup;
use crate::layout::{LayoutElement as _, LayoutElementRenderElement};
use crate::render_helpers::clipped_surface::ClippedSurfaceRenderElement;
use crate::render_helpers::mesh::{MeshRenderElement};
use crate::render_helpers::offscreen::{NativeOffscreenBuffer, OffscreenBuffer};
use crate::render_helpers::renderer::ClarionRenderer;
use crate::render_helpers::vulkan::VulkanTexture;
use crate::render_helpers::vulkan::native::NativeRenderer;
use crate::render_helpers::RenderCtx;
use crate::window::Mapped;

use super::mesh::card_mesh_area;
use super::{StageManagerUi, StageRailPlan};

clarion_render_elements! {
    StageRailRenderElement<R> => {
        Thumbnail = RelocateRenderElement<RescaleRenderElement<LayoutElementRenderElement<R>>>,
        ClippedThumbnail =
            RelocateRenderElement<RescaleRenderElement<ClippedSurfaceRenderElement<R>>>,
        Mesh = MeshRenderElement,
    }
}

clarion_render_elements! {
    StageCardElement<R> => {
        Element = RelocateRenderElement<RescaleRenderElement<LayoutElementRenderElement<R>>>,
        Clipped = RelocateRenderElement<RescaleRenderElement<ClippedSurfaceRenderElement<R>>>,
    }
}

/// Persistent offscreen card texture: the window content + rounded clip is
/// rendered once per frame, then sampled by the perspective mesh.
#[derive(Debug)]
pub(super) struct CardTexture {
    pub(super) core_id: u64,
    pub(super) id: Id,
    pub(super) commit: CommitCounter,
    pub(super) gles: OffscreenBuffer,
    pub(super) native: NativeOffscreenBuffer,
    /// Last rendered GLES sampling source, reused until the content or the
    /// card slot changes.
    pub(super) last_gles: Option<(GlesTexture, Size<f64, smithay::utils::Buffer>)>,
    /// Last rendered Vulkan sampling source.
    pub(super) last_native: Option<(VulkanTexture, Size<f64, smithay::utils::Buffer>)>,
    pub(super) rendered_at: Option<std::time::Instant>,
    pub(super) rendered_size: Option<Size<f64, Logical>>,
    pub(super) rendered_scale: Option<f64>,
}

impl StageManagerUi {
    /// Render the rail. `windows` resolves core identities to live windows.
    pub(crate) fn render_output<R: ClarionRenderer>(
        &self,
        plan: &StageRailPlan,
        output: &Output,
        windows: &dyn WindowLookup<Mapped>,
        mut ctx: RenderCtx<R>,
        push: &mut dyn FnMut(StageRailRenderElement<R>),
    ) {
        if plan.is_empty() {
            return;
        }
        let output_scale = output.current_scale().fractional_scale();

        if let Some(mut gles) = ctx.as_gles() {
            if MeshRenderElement::is_available(gles.renderer) {
                self.render_cards_gles(plan, output_scale, windows, &mut gles, push);
                return;
            }
        }
        if let Some(mut native) = ctx.as_native() {
            self.render_cards_native(plan, output_scale, windows, &mut native, push);
            return;
        }
        // Renderers without mesh support fall back to a flat, clipped card.
        self.render_cards_flat(plan, output_scale, windows, &mut ctx, push);
    }

    fn render_cards_gles<R: ClarionRenderer>(
        &self,
        plan: &StageRailPlan,
        output_scale: f64,
        windows: &dyn WindowLookup<Mapped>,
        ctx: &mut RenderCtx<'_, GlesRenderer>,
        push: &mut dyn FnMut(StageRailRenderElement<R>),
    ) {
        for card in &plan.cards {
            let Some(mapped) = windows.window(card.core_id) else {
                continue;
            };
            let Some((area, vertices)) = card_mesh_area(card, self.tuning.shear) else {
                continue;
            };
            let slot = card.geometry.size;
            self.ensure_texture(card.core_id);
            let mut textures = self.textures.borrow_mut();
            let Some(texture) = textures
                .iter_mut()
                .find(|texture| texture.core_id == card.core_id)
            else {
                continue;
            };
            let stale = texture.last_gles.is_none()
                || texture.rendered_size != Some(slot)
                || texture.rendered_scale != Some(output_scale)
                || texture.rendered_at.is_none_or(|at| {
                    at.elapsed().as_millis() as u64 >= self.tuning.texture_interval_ms
                });
            if stale {
                let elements = collect_card_elements(
                    ctx,
                    mapped,
                    slot,
                    output_scale,
                    self.tuning.radius,
                );
                if elements.is_empty() {
                    continue;
                }
                let rendered = texture
                    .gles
                    .render(ctx.renderer, Scale::from(output_scale), &elements);
                let (rendered, _sync, _data) = match rendered {
                    Ok(rendered) => rendered,
                    Err(error) => {
                        tracing::warn!("stage card offscreen render failed: {error:?}");
                        continue;
                    }
                };
                texture.last_gles = Some((rendered.texture().clone(), rendered.src().size));
                texture.rendered_at = Some(std::time::Instant::now());
                texture.rendered_size = Some(slot);
                texture.rendered_scale = Some(output_scale);
                texture.commit.increment();
            }
            let Some((texture_id, src)) = &texture.last_gles else {
                continue;
            };
            push(StageRailRenderElement::Mesh(MeshRenderElement::new(
                texture.id.clone(),
                texture.commit,
                texture_id.clone(),
                *src,
                area,
                vertices,
                plan.alpha * card.alpha,
                Kind::Unspecified,
            )));
        }
    }

    fn render_cards_native<R: ClarionRenderer>(
        &self,
        plan: &StageRailPlan,
        output_scale: f64,
        windows: &dyn WindowLookup<Mapped>,
        ctx: &mut RenderCtx<'_, NativeRenderer>,
        push: &mut dyn FnMut(StageRailRenderElement<R>),
    ) {
        for card in &plan.cards {
            let Some(mapped) = windows.window(card.core_id) else {
                continue;
            };
            let Some((area, vertices)) = card_mesh_area(card, self.tuning.shear) else {
                continue;
            };
            let slot = card.geometry.size;
            self.ensure_texture(card.core_id);
            let mut textures = self.textures.borrow_mut();
            let Some(texture) = textures
                .iter_mut()
                .find(|texture| texture.core_id == card.core_id)
            else {
                continue;
            };
            let stale = texture.last_native.is_none()
                || texture.rendered_size != Some(slot)
                || texture.rendered_scale != Some(output_scale)
                || texture.rendered_at.is_none_or(|at| {
                    at.elapsed().as_millis() as u64 >= self.tuning.texture_interval_ms
                });
            if stale {
                let elements = collect_card_elements(
                    ctx,
                    mapped,
                    slot,
                    output_scale,
                    self.tuning.radius,
                );
                if elements.is_empty() {
                    continue;
                }
                let rendered = texture
                    .native
                    .render(ctx.renderer, Scale::from(output_scale), &elements);
                let (rendered, _sync, geometry, _states, _commit) = match rendered {
                    Ok(rendered) => rendered,
                    Err(error) => {
                        tracing::warn!("stage card native offscreen render failed: {error:?}");
                        continue;
                    }
                };
                let used =
                    Size::from((f64::from(geometry.size.w), f64::from(geometry.size.h)));
                texture.last_native = Some((rendered, used));
                texture.rendered_at = Some(std::time::Instant::now());
                texture.rendered_size = Some(slot);
                texture.rendered_scale = Some(output_scale);
                texture.commit.increment();
            }
            let Some((texture_id, used)) = &texture.last_native else {
                continue;
            };
            push(StageRailRenderElement::Mesh(
                MeshRenderElement::new_vulkan(
                    texture.id.clone(),
                    texture.commit,
                    texture_id.clone(),
                    *used,
                    area,
                    vertices,
                    plan.alpha * card.alpha,
                    Kind::Unspecified,
                ),
            ));
        }
    }

    fn render_cards_flat<R: ClarionRenderer>(
        &self,
        plan: &StageRailPlan,
        output_scale: f64,
        windows: &dyn WindowLookup<Mapped>,
        ctx: &mut RenderCtx<'_, R>,
        push: &mut dyn FnMut(StageRailRenderElement<R>),
    ) {
        let clip_shader = ClippedSurfaceRenderElement::clip_program(ctx.renderer);
        for card in &plan.cards {
            let Some(mapped) = windows.window(card.core_id) else {
                continue;
            };
            let size = mapped.size().to_f64();
            if size.w < 1. || size.h < 1. {
                continue;
            }
            let geo = card.geometry;
            if geo.size.w < 1. || geo.size.h < 1. {
                continue;
            }
            // Without a mesh, fit the window into the slot and apply a flat
            // horizontal approximation of the rotation.
            let flat = card.angle.cos().abs().clamp(0.02, 1.);
            let fit = (geo.size.w / size.w).min(geo.size.h / size.h).max(0.01);
            let thumb = Scale::from(fit * flat);
            let content: Size<f64, Logical> = Size::from((size.w * thumb.x, size.h * thumb.y));
            let offset = Point::from((
                (geo.size.w - content.w) / 2.,
                (geo.size.h - content.h) / 2.,
            ));
            let window_geo = Rectangle::from_size(size);
            let radius = CornerRadius::from((self.tuning.radius / thumb.y.max(f64::EPSILON)) as f32);
            let location = (geo.loc + offset).to_physical_precise_round(output_scale);
            mapped.render_normal(
                ctx.r(),
                Point::from((0., 0.)),
                Scale::from(output_scale),
                plan.alpha * card.alpha,
                &mut |elem| match elem {
                    LayoutElementRenderElement::Wayland(elem) => {
                        let clip = clip_shader.clone().filter(|_| {
                            ClippedSurfaceRenderElement::will_clip(
                                &elem,
                                Scale::from(output_scale),
                                window_geo,
                                radius,
                            )
                        });
                        match clip {
                            Some(shader) => {
                                let elem = ClippedSurfaceRenderElement::new(
                                    elem,
                                    Scale::from(output_scale),
                                    window_geo,
                                    shader,
                                    radius,
                                );
                                let elem = RescaleRenderElement::from_element(
                                    elem,
                                    Point::<i32, smithay::utils::Physical>::from((0, 0)),
                                    thumb,
                                );
                                push(StageRailRenderElement::ClippedThumbnail(
                                    RelocateRenderElement::from_element(
                                        elem,
                                        location,
                                        Relocate::Relative,
                                    ),
                                ));
                            }
                            None => {
                                let elem = RescaleRenderElement::from_element(
                                    LayoutElementRenderElement::Wayland(elem),
                                    Point::<i32, smithay::utils::Physical>::from((0, 0)),
                                    thumb,
                                );
                                push(StageRailRenderElement::Thumbnail(
                                    RelocateRenderElement::from_element(
                                        elem,
                                        location,
                                        Relocate::Relative,
                                    ),
                                ));
                            }
                        }
                    }
                    other => {
                        let elem = RescaleRenderElement::from_element(
                            other,
                            Point::<i32, smithay::utils::Physical>::from((0, 0)),
                            thumb,
                        );
                        push(StageRailRenderElement::Thumbnail(
                            RelocateRenderElement::from_element(
                                elem,
                                location,
                                Relocate::Relative,
                            ),
                        ));
                    }
                },
            );
        }
    }

    fn ensure_texture(&self, core_id: u64) {
        let mut textures = self.textures.borrow_mut();
        if !textures.iter().any(|texture| texture.core_id == core_id) {
            textures.push(CardTexture {
                core_id,
                id: Id::new(),
                commit: CommitCounter::default(),
                gles: OffscreenBuffer::default(),
                native: NativeOffscreenBuffer::default(),
                last_gles: None,
                last_native: None,
                rendered_at: None,
                rendered_size: None,
                rendered_scale: None,
            });
        }
    }

}

/// Collect the card content scaled to the card size, clipped to rounded
/// corners, for an offscreen render.
fn collect_card_elements<R: ClarionRenderer>(
    ctx: &mut RenderCtx<'_, R>,
    mapped: &Mapped,
    slot: Size<f64, Logical>,
    output_scale: f64,
    corner_radius: f64,
) -> Vec<StageCardElement<R>> {
    let size = mapped.size().to_f64();
    if size.w < 1. || size.h < 1. || slot.w < 1. || slot.h < 1. {
        return Vec::new();
    }
    // The card is the window at a common scale, so the content fills it.
    let fit = (slot.w / size.w).min(slot.h / size.h).max(0.01);
    let thumb = Scale::from(fit);
    let content: Size<f64, Logical> = Size::from((size.w * fit, size.h * fit));
    let offset = Point::from((
        ((slot.w - content.w) / 2.).round() as i32,
        ((slot.h - content.h) / 2.).round() as i32,
    ));
    let window_geo = Rectangle::from_size(size);
    let radius = CornerRadius::from((corner_radius / fit.max(f64::EPSILON)) as f32);
    let clip_shader = ClippedSurfaceRenderElement::clip_program(ctx.renderer);
    let mut elements = Vec::new();
    mapped.render_normal(
        ctx.r(),
        Point::from((0., 0.)),
        Scale::from(output_scale),
        1.,
        &mut |elem| match elem {
            LayoutElementRenderElement::Wayland(elem) => {
                let clip = clip_shader.clone().filter(|_| {
                    ClippedSurfaceRenderElement::will_clip(
                        &elem,
                        Scale::from(output_scale),
                        window_geo,
                        radius,
                    )
                });
                match clip {
                    Some(shader) => {
                        let elem = ClippedSurfaceRenderElement::new(
                            elem,
                            Scale::from(output_scale),
                            window_geo,
                            shader,
                            radius,
                        );
                        let elem = RescaleRenderElement::from_element(
                            elem,
                            Point::<i32, smithay::utils::Physical>::from((0, 0)),
                            thumb,
                        );
                        elements.push(StageCardElement::Clipped(
                            RelocateRenderElement::from_element(
                                elem,
                                offset,
                                Relocate::Relative,
                            ),
                        ));
                    }
                    None => {
                        let elem = RescaleRenderElement::from_element(
                            LayoutElementRenderElement::Wayland(elem),
                            Point::<i32, smithay::utils::Physical>::from((0, 0)),
                            thumb,
                        );
                        elements.push(StageCardElement::Element(
                            RelocateRenderElement::from_element(
                                elem,
                                offset,
                                Relocate::Relative,
                            ),
                        ));
                    }
                }
            }
            other => {
                let elem = RescaleRenderElement::from_element(
                    other,
                    Point::<i32, smithay::utils::Physical>::from((0, 0)),
                    thumb,
                );
                elements.push(StageCardElement::Element(
                    RelocateRenderElement::from_element(elem, offset, Relocate::Relative),
                ));
            }
        },
    );
    elements
}

