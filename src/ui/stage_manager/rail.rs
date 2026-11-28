//! Rail layout: the short centred stack and the overflowing ring, plus the
//! reveal cascade that slides cards out one after another.

use smithay::utils::{Logical, Point, Rectangle, Size};


/// Every tunable of the rail in one place: geometry, perspective and timing.
///
/// `StageManagerUi` owns one of these, so a future preferences page (or a
/// test) can change the look without touching the state machine.
#[derive(Debug, Clone, Copy)]
pub struct StageRailTuning {
    /// Pointer distance from the edge that reveals the rail.
    pub reveal_px: f64,
    /// Distance past the rail that closes it again.
    pub close_pad: f64,
    /// Resting distance between the rail and the output edge.
    pub inset: f64,
    pub gap: f64,
    /// A full-height window maps to this fraction of the working area.
    pub height_fraction: f64,
    pub base_min: f64,
    pub base_max: f64,
    /// Once the strip is taller than this fraction of the working area it
    /// scrolls instead of shrinking every card.
    pub viewport_fraction: f64,
    /// One wheel notch or arrow key, as a fraction of a card slot.
    pub scroll_fraction: f64,
    /// Visual corner radius of a rail card.
    pub radius: f64,
    /// Rail cards rest turned into the screen (right edge receding).
    pub resting_angle: f64,
    /// Vertical shear that makes the whole strip read as one rotated plane.
    pub shear: f64,
    /// Pointer hover only turns the card partway back to the viewer.
    pub hover_turn: f64,
    /// Extra size of a hovered card, in logical pixels.
    pub hover_lift: f64,
    /// Delay between neighbouring cards in the reveal cascade.
    pub stagger: f64,
    pub stagger_max: f64,
    pub reveal_ms: u64,
    pub flip_ms: u64,
    pub fly_ms: u64,
    pub hover_ms: u64,
    pub scroll_ms: u64,
    /// Minimum time between offscreen card-texture redraws while the rail is
    /// open. The mesh still moves every frame; only the window content is
    /// refreshed at this cadence.
    pub texture_interval_ms: u64,
}

impl Default for StageRailTuning {
    fn default() -> Self {
        Self {
            reveal_px: 6.,
            close_pad: 40.,
            inset: 10.,
            gap: 16.,
            height_fraction: 0.10,
            base_min: 56.,
            base_max: 176.,
            viewport_fraction: 0.70,
            scroll_fraction: 0.75,
            radius: 12.,
            resting_angle: -0.62,
            shear: 0.22,
            hover_turn: 0.45,
            hover_lift: 4.,
            stagger: 0.20,
            stagger_max: 0.65,
            reveal_ms: 520,
            flip_ms: 260,
            fly_ms: 340,
            hover_ms: 170,
            scroll_ms: 260,
            texture_interval_ms: 100,
        }
    }
}

pub(super) fn grow(rect: Rectangle<f64, Logical>, amount: f64) -> Rectangle<f64, Logical> {
    Rectangle::new(
        rect.loc - Point::from((amount / 2., amount / 2.)),
        rect.size + Size::from((amount, amount)),
    )
}

pub(super) struct RailGeometry {
    pub(super) cards: Vec<(u64, Rectangle<f64, Logical>, f32)>,
    /// Whether the strip is taller than its viewport and can scroll.
    pub(super) scrolling: bool,
    /// One wheel notch / arrow-key step, in logical pixels.
    pub(super) step: f64,
    /// Right edge of the widest card, used for the reveal/close zone.
    pub(super) rail_right: f64,
}

/// Map `value` into `[-total/2, total/2)`, the ring around the viewport.
fn wrap_signed(value: f64, total: f64) -> f64 {
    if total <= 0. {
        return 0.;
    }
    (value + total / 2.).rem_euclid(total) - total / 2.
}

/// Stack the window thumbnails in the left rail, sliding them out by
/// `reveal`. A strip taller than 70% of the working area becomes an infinite
/// ring scrolled by `scroll`; cards crossing the viewport edges fade out.
pub(super) fn rail_geometry(
    area: Rectangle<f64, Logical>,
    reveal: f64,
    windows: &[(u64, Size<f64, Logical>)],
    scroll: f64,
    tuning: &StageRailTuning,
) -> RailGeometry {
    if windows.is_empty() || area.size.w <= 1. || area.size.h <= 1. {
        return RailGeometry {
            cards: Vec::new(),
            scrolling: false,
            step: 64.,
            rail_right: 0.,
        };
    }
    let count = windows.len();
    // One common scale for every window: a window at most as large as the
    // screen maps to a screen-sized card, smaller windows stay smaller.
    let base = (area.size.h * tuning.height_fraction).clamp(tuning.base_min, tuning.base_max);
    let scale = base / area.size.h.max(1.);
    let sizes: Vec<(u64, Size<f64, Logical>)> = windows
        .iter()
        .map(|(id, window)| (*id, window.downscale(1. / scale)))
        .collect();

    let content: f64 = sizes.iter().map(|(_, size)| size.h).sum::<f64>()
        + tuning.gap * count.saturating_sub(1) as f64;
    // A ring period has one more gap: the seam keeps the same spacing instead
    // of butting the head straight into the tail.
    let total = content + tuning.gap;
    let viewport = (area.size.h * tuning.viewport_fraction).max(40.);
    let scrolling = content > viewport + 0.5;
    let step = (base + tuning.gap) * tuning.scroll_fraction;

    let reveal = reveal.clamp(0., 1.);
    // Cards slide out like a ribbon: at any moment the first card is
    // furthest out, the next a step less, and so on. Every card still starts
    // before the previous one reaches its end and all converge on the fully
    // open position. Long strips spread the same total lag over every card so
    // the tail still moves instead of stacking up behind the cap.
    let stagger = if count > 1 {
        (tuning.stagger_max / (count - 1) as f64).min(tuning.stagger)
    } else {
        0.
    };

    let center = area.loc.y + area.size.h / 2.;
    let top = center - viewport / 2.;
    let bottom = center + viewport / 2.;
    // A soft edge inside the viewport: cards reaching the top/bottom fade
    // before they slide out, signalling the ring continues past the edge.
    let fade = (viewport * 0.09).clamp(24., 72.);
    // The ring's zero sits half a viewport above the centre, so scroll 0
    // shows the MRU (first) card at the top of the viewport instead of the
    // oldest wrapped card. Wrapping happens at ±total/2, well outside the
    // viewport, so the seam never pops on screen.
    let scroll = if scrolling {
        scroll.rem_euclid(total) + viewport / 2.
    } else {
        0.
    };

    // A short strip is a plain centred stack in MRU order. Only an
    // overflowing one becomes a ring.
    let stack_top = center - content / 2.;
    let mut cards = Vec::with_capacity(count * 2);
    let mut offset = 0.;
    for (index, (id, size)) in sizes.into_iter().enumerate() {
        let card_height = size.h;
        let card_top = offset;
        let mid = offset + card_height / 2.;
        offset += card_height + tuning.gap;
        let lag = stagger * index as f64;
        let duration = (1. - lag).max(0.05);
        // Smoothstep each card locally so every card starts and ends at zero
        // velocity; the gradient lag then reads as one flowing ribbon.
        let local = ((reveal - lag) / duration).clamp(0., 1.);
        let progress = local * local * (3. - 2. * local);
        if progress <= 0.01 {
            continue;
        }
        // Each card travels its own width, so its visible part grows with
        // its progress instead of a narrow card staying hidden behind a
        // wide card's slot.
        let slide = (tuning.inset + size.w) * (1. - progress);
        let x = area.loc.x + tuning.inset - slide;

        let mut placements: [Option<(f64, f64)>; 3] = [None; 3];
        if scrolling {
            // The ring sits around the viewport centre. Each card is also
            // placed one period above and below so the seam stays covered.
            let ring = mid - scroll;
            let base_pos = wrap_signed(ring, total);
            for (slot, shift) in placements.iter_mut().zip([0., -total, total]) {
                let y = center + base_pos + shift - card_height / 2.;
                let visible = (y + card_height).min(bottom) - y.max(top);
                if visible <= 0.5 {
                    continue;
                }
                // Fade by how much of the card actually entered the viewport:
                // a sliver at the edge is faint, a full card is opaque.
                *slot = Some((y, (visible / fade).clamp(0., 1.)));
            }
        } else {
            placements[0] = Some((stack_top + card_top, 1.));
        }

        for (y, edge_alpha) in placements.into_iter().flatten() {
            let alpha = (edge_alpha * progress) as f32;
            if alpha <= 0.01 {
                continue;
            }
            cards.push((
                id,
                Rectangle::new(
                    Point::from((x, y)),
                    Size::from((size.w, card_height)),
                ),
                alpha,
            ));
        }
    }
    let rail_right = cards
        .iter()
        .map(|(_, rect, _)| rect.loc.x + rect.size.w)
        .fold(area.loc.x + tuning.inset, f64::max);
    RailGeometry {
        cards,
        scrolling,
        step,
        rail_right,
    }
}

