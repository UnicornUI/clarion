//! macOS-style Stage Manager rail.
//!
//! The rail is a floating, compositor-owned strip on the left edge of an
//! output. It stays hidden until the pointer approaches the edge, then slides
//! out with live thumbnails of the windows that Stage Manager collected from
//! the active workspace. Clicking a thumbnail activates that window.
//!
//! Nothing here transforms window geometry: the focused window is rendered by
//! the ordinary workspace composition at its real size, so client input stays
//! pixel-exact.
//!
//! The implementation is split by responsibility:
//! - `rail`: the tuning table, reveal cascade and stack/ring geometry;
//! - `mesh`: the pure card projection;
//! - `render`: offscreen card textures and the mesh render path;
//! - `tests`: geometry checks that need no GPU or compositor state.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::animation::{Animation, Clock, Curve};
use crate::layout::{Layout, LayoutElement as _};
use crate::window::Mapped;

use rail::{grow, rail_geometry};
use render::CardTexture;

mod mesh;
mod rail;
mod render;
#[cfg(test)]
mod tests;

pub use rail::StageRailTuning;
pub(crate) use render::StageRailRenderElement;

/// Edge-on pose used when a card folds in or out.
const EDGE_ANGLE: f64 = std::f64::consts::FRAC_PI_2;

/// One clickable thumbnail, already positioned for the current reveal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StageRailCard {
    pub(crate) core_id: u64,
    pub(crate) geometry: Rectangle<f64, Logical>,
    /// Rotation around the vertical axis, in radians: 0 faces the viewer,
    /// positive/negative values turn the card into the screen, ±π/2 is edge-on.
    pub(crate) angle: f64,
    /// Fade applied when the card crosses the scrolling viewport edge.
    pub(crate) alpha: f32,
}

/// Owned, per-frame description of the rail for one output.
#[derive(Debug, Clone, Default)]
pub(crate) struct StageRailPlan {
    pub(crate) alpha: f32,
    pub(crate) cards: Vec<StageRailCard>,
}

impl StageRailPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.alpha <= 0.01 || self.cards.is_empty()
    }
}

#[derive(Debug)]
struct ClosingCard {
    core_id: u64,
    geometry: Rectangle<f64, Logical>,
    anim: Animation,
}

/// One side of a stage swap, animated between the stage and the rail.
#[derive(Debug)]
struct FlyingCard {
    core_id: u64,
    from: Rectangle<f64, Logical>,
    to: Rectangle<f64, Logical>,
    angle_from: f64,
    angle_to: f64,
}

#[derive(Debug)]
struct OutputState {
    reveal: f64,
    target: f64,
    reveal_anim: Option<Animation>,
    flips: HashMap<u64, Animation>,
    closing: Vec<ClosingCard>,
    flying: Vec<FlyingCard>,
    /// Shared clock for both sides of a stage swap.
    swap: Option<Animation>,
    /// Per-card hover flip (0 = turned into the screen, 1 = facing you).
    hover_flips: HashMap<u64, Animation>,
    /// Per-card hover lift, 0 = resting in its slot, 1 = fully lifted.
    hover_lifts: HashMap<u64, Animation>,
    /// Rail order, most recently used first.
    mru: Vec<u64>,
    known: HashSet<u64>,
    /// Last computed geometry, used for hit testing between frames.
    cards: Vec<StageRailCard>,
    rail_right: f64,
    hover: Option<u64>,
    /// Vertical scroll of an overflowing strip, in logical pixels. `scroll`
    /// is the animated position; `scroll_target` accumulates wheel notches.
    scroll: f64,
    scroll_target: f64,
    scroll_anim: Option<Animation>,
    scrollable: bool,
    scroll_step: f64,
}

impl OutputState {
    fn new() -> Self {
        Self {
            reveal: 0.,
            target: 0.,
            reveal_anim: None,
            flips: HashMap::new(),
            closing: Vec::new(),
            flying: Vec::new(),
            swap: None,
            hover_flips: HashMap::new(),
            hover_lifts: HashMap::new(),
            mru: Vec::new(),
            known: HashSet::new(),
            cards: Vec::new(),
            rail_right: 0.,
            hover: None,
            scroll: 0.,
            scroll_target: 0.,
            scroll_anim: None,
            scrollable: false,
            scroll_step: 64.,
        }
    }
}

#[derive(Debug)]
pub struct StageManagerUi {
    clock: Clock,
    /// Look and timing of the rail, kept together so it can be swapped
    /// without touching the state machine.
    tuning: StageRailTuning,
    /// When true (animations disabled), transitions snap to their target.
    instant: bool,
    outputs: RefCell<HashMap<Output, OutputState>>,
    textures: RefCell<Vec<CardTexture>>,
}

impl StageManagerUi {
    pub fn new(clock: Clock) -> Self {
        Self::with_tuning(clock, StageRailTuning::default())
    }

    /// Build the rail with an explicit tuning (tests, or a future
    /// preferences page driving the compositor).
    pub fn with_tuning(clock: Clock, tuning: StageRailTuning) -> Self {
        Self {
            clock,
            tuning,
            instant: false,
            outputs: RefCell::new(HashMap::new()),
            textures: RefCell::new(Vec::new()),
        }
    }

    pub fn tuning(&self) -> StageRailTuning {
        self.tuning
    }

    pub fn set_tuning(&mut self, tuning: StageRailTuning) {
        self.tuning = tuning;
    }

    pub fn update_config(&mut self, animations_off: bool) {
        self.instant = animations_off;
        if animations_off {
            let mut outputs = self.outputs.borrow_mut();
            for state in outputs.values_mut() {
                state.reveal = state.target;
                state.reveal_anim = None;
                state.flips.clear();
                state.closing.clear();
                state.flying.clear();
                state.swap = None;
                state.hover_flips.clear();
                state.hover_lifts.clear();
                state.scroll = state.scroll_target;
                state.scroll_anim = None;
            }
        }
    }

    fn ease(&self, from: f64, to: f64, duration_ms: u64) -> Option<Animation> {
        (!self.instant && (from - to).abs() > f64::EPSILON).then(|| {
            Animation::ease(
                self.clock.clone(),
                from,
                to,
                0.,
                duration_ms,
                Curve::MACOS_EASE_IN_EASE_OUT,
            )
        })
    }

    pub fn remove_output(&mut self, output: &Output) {
        self.outputs.borrow_mut().remove(output);
    }

    /// The pointer moved inside `output`. Returns whether a redraw is needed.
    pub fn pointer_moved(&mut self, output: &Output, pos: Point<f64, Logical>) -> bool {
        let mut outputs = self.outputs.borrow_mut();
        let state = outputs
            .entry(output.clone())
            .or_insert_with(OutputState::new);
        let was_open = state.target > 0.5;
        let should_open = if pos.x <= self.tuning.reveal_px {
            true
        } else if was_open {
            pos.x <= state.rail_right.max(self.tuning.reveal_px) + self.tuning.close_pad
        } else {
            false
        };
        let target = if should_open { 1. } else { 0. };
        let mut changed = false;
        if state.target != target {
            state.target = target;
            state.reveal_anim = self
                .ease(state.reveal, target, self.tuning.reveal_ms)
                .or_else(|| {
                    // Animations disabled: snap.
                    state.reveal = target;
                    None
                });
            changed = true;
        }
        // Remember the pointer for hover highlighting in `plan`.
        let hover = state
            .cards
            .iter()
            .find(|card| card.geometry.contains(pos))
            .map(|card| card.core_id);
        if state.hover != hover {
            let previous = state.hover;
            state.hover = hover;
            changed = true;
            if !self.instant {
                if let Some(previous) = previous {
                    let from = state
                        .hover_flips
                        .get(&previous)
                        .map(|anim| anim.clamped_value())
                        .unwrap_or(self.tuning.hover_turn);
                    state.hover_flips.insert(
                        previous,
                        Animation::ease(
                            self.clock.clone(),
                            from,
                            0.,
                            0.,
                            self.tuning.hover_ms,
                            Curve::MACOS_EASE_IN_EASE_OUT,
                        ),
                    );
                    let from_lift = state
                        .hover_lifts
                        .get(&previous)
                        .map(|anim| anim.clamped_value())
                        .unwrap_or(1.);
                    state.hover_lifts.insert(
                        previous,
                        Animation::ease(
                            self.clock.clone(),
                            from_lift,
                            0.,
                            0.,
                            self.tuning.hover_ms,
                            Curve::MACOS_EASE_IN_EASE_OUT,
                        ),
                    );
                }
                if let Some(hover) = hover {
                    let from = state
                        .hover_flips
                        .get(&hover)
                        .map(|anim| anim.clamped_value())
                        .unwrap_or(0.);
                    state.hover_flips.insert(
                        hover,
                        Animation::ease(
                            self.clock.clone(),
                            from,
                            self.tuning.hover_turn,
                            0.,
                            self.tuning.hover_ms,
                            Curve::MACOS_EASE_IN_EASE_OUT,
                        ),
                    );
                    let from_lift = state
                        .hover_lifts
                        .get(&hover)
                        .map(|anim| anim.clamped_value())
                        .unwrap_or(0.);
                    state.hover_lifts.insert(
                        hover,
                        Animation::ease(
                            self.clock.clone(),
                            from_lift,
                            1.,
                            0.,
                            self.tuning.hover_ms,
                            Curve::MACOS_EASE_IN_EASE_OUT,
                        ),
                    );
                }
            }
        }
        changed
    }

    /// The pointer left every output: close the rails.
    pub fn pointer_left(&mut self) -> bool {
        let mut outputs = self.outputs.borrow_mut();
        let mut changed = false;
        for state in outputs.values_mut() {
            if state.target != 0. {
                state.target = 0.;
                state.reveal_anim = self.ease(state.reveal, 0., self.tuning.reveal_ms);
                if self.instant {
                    state.reveal = 0.;
                }
                changed = true;
            }
            changed |= self.release_hover(state);
        }
        changed
    }

    pub fn clear_hover(&mut self, output: &Output) -> bool {
        let mut outputs = self.outputs.borrow_mut();
        let Some(state) = outputs.get_mut(output) else {
            return false;
        };
        self.release_hover(state)
    }

    /// Start the smooth return of the currently hovered card. Using the
    /// animation maps keeps the angle and the lift continuous even when the
    /// pointer leaves the output entirely.
    fn release_hover(&self, state: &mut OutputState) -> bool {
        let Some(hovered) = state.hover.take() else {
            return false;
        };
        if !self.instant {
            let angle = state
                .hover_flips
                .get(&hovered)
                .map(|anim| anim.clamped_value())
                .unwrap_or(self.tuning.hover_turn);
            state.hover_flips.insert(
                hovered,
                Animation::ease(
                    self.clock.clone(),
                    angle,
                    0.,
                    0.,
                    self.tuning.hover_ms,
                    Curve::MACOS_EASE_IN_EASE_OUT,
                ),
            );
            let lift = state
                .hover_lifts
                .get(&hovered)
                .map(|anim| anim.clamped_value())
                .unwrap_or(1.);
            state.hover_lifts.insert(
                hovered,
                Animation::ease(
                    self.clock.clone(),
                    lift,
                    0.,
                    0.,
                    self.tuning.hover_ms,
                    Curve::MACOS_EASE_IN_EASE_OUT,
                ),
            );
        }
        true
    }

    pub fn hovered(&self, output: &Output) -> Option<u64> {
        self.outputs
            .borrow()
            .get(output)
            .and_then(|state| state.hover)
    }

    /// Whether the strip overflows its viewport and accepts scrolling.
    pub(crate) fn scrollable(&self, output: &Output) -> bool {
        self.outputs
            .borrow()
            .get(output)
            .is_some_and(|state| state.scrollable)
    }

    /// One wheel notch or arrow-key step, in logical pixels.
    pub(crate) fn scroll_step(&self, output: &Output) -> f64 {
        self.outputs
            .borrow()
            .get(output)
            .map(|state| state.scroll_step.max(1.))
            .unwrap_or(64.)
    }

    /// Scroll an overflowing strip. Positive `delta` advances the strip (the
    /// cards move up); the ring wraps at either end. Returns whether the
    /// scroll was accepted.
    pub(crate) fn scroll_by(&self, output: &Output, delta: f64) -> bool {
        if delta == 0. {
            return false;
        }
        let mut outputs = self.outputs.borrow_mut();
        let Some(state) = outputs.get_mut(output) else {
            return false;
        };
        if !state.scrollable {
            return false;
        }
        state.scroll_target += delta;
        if self.instant {
            state.scroll = state.scroll_target;
            state.scroll_anim = None;
        } else {
            state.scroll_anim = Some(Animation::ease(
                self.clock.clone(),
                state.scroll,
                state.scroll_target,
                0.,
                self.tuning.scroll_ms,
                Curve::MACOS_EASE_OUT,
            ));
        }
        true
    }

    /// The rect of a card currently shown in the rail.
    pub fn card_rect(&self, output: &Output, core_id: u64) -> Option<Rectangle<f64, Logical>> {
        self.outputs
            .borrow()
            .get(output)?
            .cards
            .iter()
            .find(|card| card.core_id == core_id)
            .map(|card| card.geometry)
    }

    /// Animate a two-sided exchange: the outgoing window folds into the rail
    /// while the clicked window grows from its card onto the stage.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_swap(
        &mut self,
        output: &Output,
        area: Rectangle<f64, Logical>,
        outgoing: (u64, Size<f64, Logical>, Rectangle<f64, Logical>),
        incoming: (u64, Size<f64, Logical>, Rectangle<f64, Logical>),
        stage: Rectangle<f64, Logical>,
        parked_after: &[(u64, Size<f64, Logical>)],
    ) {
        let mut outputs = self.outputs.borrow_mut();
        let state = outputs.entry(output.clone()).or_insert_with(OutputState::new);
        if self.instant {
            state.flying.clear();
            state.swap = None;
            return;
        }
        // The outgoing window is already part of the collected list by now;
        // keep exactly one entry so the flight lands on the resting slot.
        let mut list = Vec::with_capacity(parked_after.len() + 1);
        list.push((outgoing.0, outgoing.1));
        list.extend(
            parked_after
                .iter()
                .copied()
                .filter(|(id, _)| *id != outgoing.0),
        );
        let Some(rail_slot) = rail_geometry(area, state.reveal, &list, state.scroll, &self.tuning)
            .cards
            .first()
            .map(|(_, rect, _)| *rect)
        else {
            return;
        };
        // Start each flight from the card's current rail pose, so a hovered
        // card does not snap flat when the exchange begins. Both sides
        // straighten as they move.
        let current_angle = |id: u64| {
            state
                .cards
                .iter()
                .find(|card| card.core_id == id)
                .map(|card| card.angle)
                .unwrap_or(0.)
        };
        let outgoing_angle = current_angle(outgoing.0);
        let incoming_angle = current_angle(incoming.0);
        state.flying.clear();
        state.flying.push(FlyingCard {
            core_id: outgoing.0,
            from: outgoing.2,
            to: rail_slot,
            angle_from: outgoing_angle,
            angle_to: 0.,
        });
        state.mru.retain(|id| *id != outgoing.0 && *id != incoming.0);
        state.mru.insert(0, outgoing.0);
        // The restored window grows out of its card and flattens onto the
        // stage.
        state.flying.push(FlyingCard {
            core_id: incoming.0,
            from: incoming.2,
            to: stage,
            angle_from: incoming_angle,
            angle_to: 0.,
        });
        // Drop stale hover tilts: the landed card must start flat and settle
        // back into the rail.
        state.hover_flips.remove(&outgoing.0);
        state.hover_flips.remove(&incoming.0);
        state.hover_lifts.remove(&outgoing.0);
        state.hover_lifts.remove(&incoming.0);
        state.swap = Some(Animation::ease(
            self.clock.clone(),
            0.,
            1.,
            0.,
            self.tuning.fly_ms,
            Curve::MACOS_EASE_IN_EASE_OUT,
        ));
        state.known.insert(outgoing.0);
        state.known.insert(incoming.0);
        state.hover = None;
    }

    pub fn swap_finished(&self, output: &Output) -> bool {
        let outputs = self.outputs.borrow();
        let Some(state) = outputs.get(output) else {
            return true;
        };
        state.swap.as_ref().is_none_or(|anim| anim.is_done())
    }

    /// Thumbnail under a position in output coordinates, when the rail is open.
    pub fn card_at(&self, output: &Output, pos: Point<f64, Logical>) -> Option<u64> {
        let outputs = self.outputs.borrow();
        let state = outputs.get(output)?;
        if state.reveal < 0.05 {
            return None;
        }
        state
            .cards
            .iter()
            .find(|card| card.geometry.contains(pos))
            .map(|card| card.core_id)
    }

    /// True when a position belongs to the rail surface, so window hit testing
    /// must not see what is beneath it.
    pub fn covers(&self, output: &Output, pos: Point<f64, Logical>) -> bool {
        let outputs = self.outputs.borrow();
        let Some(state) = outputs.get(output) else {
            return false;
        };
        state.reveal > 0.05
            && pos.x <= state.rail_right.max(0.) + self.tuning.inset
            && pos.x >= 0.
    }

    pub fn reveal_amount(&self, output: &Output) -> f64 {
        self.outputs
            .borrow()
            .get(output)
            .map(|state| state.reveal)
            .unwrap_or(0.)
    }

    /// Build the current rail description for an output.
    ///
    /// Also starts the flip-in animation of newly collected windows.
    pub(crate) fn plan(
        &self,
        output: &Output,
        layout: &Layout<Mapped>,
        instant: bool,
    ) -> StageRailPlan {
        let Some(monitor) = layout.monitor_for_output(output) else {
            return StageRailPlan::default();
        };
        let area = monitor.working_area();
        // One walk over the collected windows yields size, focus order and
        // the MRU rank; the old version walked the list three times and built
        // a temporary rect map on every output frame.
        let mut windows: Vec<(u64, Size<f64, Logical>, Option<std::time::Duration>)> = layout
            .stage_manager_windows(output)
            .into_iter()
            .map(|window| {
                let size = monitor
                    .active_workspace_ref()
                    .window_render_rect(&window.window)
                    .map(|rect| rect.size)
                    .unwrap_or_else(|| window.size().to_f64());
                (window.core_id(), size, window.get_focus_timestamp())
            })
            .collect();
        // Most recently focused first, matching macOS' stage strip.
        windows.sort_by_key(|(_, _, timestamp)| std::cmp::Reverse(*timestamp));

        let mut outputs = self.outputs.borrow_mut();
        let state = outputs
            .entry(output.clone())
            .or_insert_with(OutputState::new);

        let mut rank: HashMap<u64, usize> = windows
            .iter()
            .enumerate()
            .map(|(index, (id, _, _))| (*id, index + state.mru.len()))
            .collect();
        for (index, id) in state.mru.iter().enumerate() {
            rank.insert(*id, index);
        }
        windows.sort_by_key(|(id, _, _)| rank.get(id).copied().unwrap_or(usize::MAX));
        let windows: Vec<(u64, Size<f64, Logical>)> = windows
            .into_iter()
            .map(|(id, size, _)| (id, size))
            .collect();

        // Advance the reveal animation to "now".
        if let Some(anim) = &state.reveal_anim {
            state.reveal = anim.clamped_value().clamp(0., 1.);
            if anim.is_done() {
                state.reveal_anim = None;
            }
        }
        // Advance the strip's scroll too. A fully hidden rail restarts at the
        // top so the next reveal never opens mid-list.
        if let Some(anim) = &state.scroll_anim {
            state.scroll = anim.clamped_value();
            if anim.is_done() {
                state.scroll_anim = None;
                state.scroll_target = state.scroll;
            }
        }
        if state.reveal <= 0.01 && state.reveal_anim.is_none() && state.scroll != 0. {
            state.scroll = 0.;
            state.scroll_target = 0.;
            state.scroll_anim = None;
        }

        // Newly collected windows flip open from the edge.
        let current: HashSet<u64> = windows.iter().map(|(id, _)| *id).collect();
        if instant {
            state.flips.clear();
        } else {
            for id in &current {
                if !state.known.contains(id) {
                    state.flips.entry(*id).or_insert_with(|| {
                        Animation::ease(
                            self.clock.clone(),
                            0.,
                            1.,
                            0.,
                            self.tuning.flip_ms,
                            Curve::MACOS_EASE_OUT,
                        )
                    });
                }
            }
            state.flips.retain(|id, anim| !anim.is_done() && current.contains(id));
        }
        state.known = current;

        // Retire finished flip-out and flight cards.
        if instant {
            state.closing.clear();
            state.flying.clear();
            state.swap = None;
        } else {
            state.closing.retain(|closing| !closing.anim.is_done());
            if state.swap.as_ref().is_some_and(|anim| anim.is_done()) {
                state.swap = None;
                // The landed card starts flat and tilts into the rail: no
                // rotation happens during the exchange itself.
                let landed: Vec<u64> = state.flying.iter().map(|f| f.core_id).collect();
                for id in landed {
                    state.hover_flips.entry(id).or_insert_with(|| {
                        Animation::ease(
                            self.clock.clone(),
                            1.,
                            0.,
                            0.,
                            self.tuning.hover_ms * 2,
                            Curve::MACOS_EASE_OUT,
                        )
                    });
                }
                state.flying.clear();
            }
        }
        let flying_ids: HashSet<u64> = state.flying.iter().map(|f| f.core_id).collect();

        let reveal = state.reveal.clamp(0., 1.);
        let geometry = rail_geometry(area, reveal, &windows, state.scroll, &self.tuning);
        state.rail_right = if reveal > 0.01 { geometry.rail_right } else { 0. };
        state.scrollable = geometry.scrolling;
        state.scroll_step = geometry.step;

        let mut cards = Vec::with_capacity(
            geometry.cards.len() + state.closing.len() + state.flying.len(),
        );
        for (core_id, rect, alpha) in geometry.cards {
            // A window in flight is drawn by its own animated rect.
            if flying_ids.contains(&core_id) {
                continue;
            }
            let open = state
                .flips
                .get(&core_id)
                .map(|anim| anim.clamped_value().clamp(0., 1.))
                .unwrap_or(1.);
            // Hover turns the thumbnail around to face the viewer.
            let hovered = state.hover == Some(core_id);
            let hover = match state.hover_flips.get(&core_id) {
                Some(anim) => anim.clamped_value().clamp(0., 1.),
                None => {
                    if hovered {
                        self.tuning.hover_turn
                    } else {
                        0.
                    }
                }
            };
            let resting = self.tuning.resting_angle * (1. - hover);
            let angle = EDGE_ANGLE * (1. - open) + resting * open;
            let lift = match state.hover_lifts.get(&core_id) {
                Some(anim) => anim.clamped_value().clamp(0., 1.),
                None => {
                    if hovered {
                        1.
                    } else {
                        0.
                    }
                }
            };
            cards.push(StageRailCard {
                core_id,
                geometry: if lift > 0.01 {
                    grow(rect, self.tuning.hover_lift * lift)
                } else {
                    rect
                },
                angle,
                alpha,
            });
        }
        for closing in &state.closing {
            // The clicked card folds away edge-on.
            let progress = closing.anim.clamped_value().clamp(0., 1.);
            cards.push(StageRailCard {
                core_id: closing.core_id,
                geometry: closing.geometry,
                angle: self.tuning.resting_angle + (EDGE_ANGLE - self.tuning.resting_angle) * (1. - progress),
                alpha: 1.,
            });
        }
        let swap_t = state
            .swap
            .as_ref()
            .map(|anim| anim.clamped_value().clamp(0., 1.))
            .unwrap_or(1.);
        for flying in &state.flying {
            let t = swap_t;
            let geometry = Rectangle::new(
                flying.from.loc + (flying.to.loc - flying.from.loc).upscale(t),
                Size::from((
                    flying.from.size.w + (flying.to.size.w - flying.from.size.w) * t,
                    flying.from.size.h + (flying.to.size.h - flying.from.size.h) * t,
                )),
            );
            let angle = flying.angle_from + (flying.angle_to - flying.angle_from) * t;
            cards.push(StageRailCard {
                core_id: flying.core_id,
                geometry,
                angle,
                alpha: 1.,
            });
        }
        // Keep the offscreen textures only for cards that are still shown;
        // otherwise every window ever collected would keep a GPU texture.
        let live: HashSet<u64> = cards.iter().map(|card| card.core_id).collect();
        self.textures
            .borrow_mut()
            .retain(|texture| live.contains(&texture.core_id));
        state.cards = cards.clone();

        StageRailPlan {
            // The per-card progress already fades each thumbnail in, so the
            // whole-rail fade only needs to cover the very start.
            alpha: (reveal * 2.2).clamp(0., 1.) as f32,
            cards,
        }
    }

    pub fn are_animations_ongoing_on(&self, output: &Output) -> bool {
        let outputs = self.outputs.borrow();
        let Some(state) = outputs.get(output) else {
            return false;
        };
        state
            .reveal_anim
            .as_ref()
            .is_some_and(|anim| !anim.is_done())
            || state.flips.values().any(|anim| !anim.is_done())
            || state
                .closing
                .iter()
                .any(|closing| !closing.anim.is_done())
            || state.swap.as_ref().is_some_and(|anim| !anim.is_done())
            || state.hover_flips.values().any(|anim| !anim.is_done())
            || state.hover_lifts.values().any(|anim| !anim.is_done())
            || state.scroll_anim.as_ref().is_some_and(|anim| !anim.is_done())
    }

    pub fn advance_animations(&mut self) {
        let mut outputs = self.outputs.borrow_mut();
        for state in outputs.values_mut() {
            state.reveal_anim.take_if(|anim| anim.is_done());
            state.flips.retain(|_, anim| !anim.is_done());
            state.closing.retain(|closing| !closing.anim.is_done());
            if state.swap.as_ref().is_some_and(|anim| anim.is_done()) {
                state.swap = None;
                // The landed card starts flat and tilts into the rail: no
                // rotation happens during the exchange itself.
                let landed: Vec<u64> = state.flying.iter().map(|f| f.core_id).collect();
                for id in landed {
                    state.hover_flips.entry(id).or_insert_with(|| {
                        Animation::ease(
                            self.clock.clone(),
                            1.,
                            0.,
                            0.,
                            self.tuning.hover_ms * 2,
                            Curve::MACOS_EASE_OUT,
                        )
                    });
                }
                state.flying.clear();
            }
            state.hover_flips.retain(|_, anim| !anim.is_done());
            state.hover_lifts.retain(|_, anim| !anim.is_done());
            if state.scroll_anim.as_ref().is_some_and(|anim| anim.is_done()) {
                state.scroll = state.scroll_target;
                state.scroll_anim = None;
            }
        }
    }

}
