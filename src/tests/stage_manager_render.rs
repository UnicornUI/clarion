use clarion_config::{Config, Workspace, WorkspaceName};
use smithay::utils::{Logical, Physical, Point};

use super::client::ClientId;
use super::stage_render::pixels;
use super::Fixture;
use crate::layout::LayoutElement as _;
use crate::render_helpers::RenderTarget;

fn add_window(f: &mut Fixture, client: ClientId, color: [u32; 4]) {
    let window = f.client(client).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.double_roundtrip(client);
    let window = f.client(client).window(&surface);
    let buffer = window.spbm.create_u32_rgba_buffer(
        color[0], color[1], color[2], color[3], &window.qh, (),
    );
    window.surface.attach(Some(&buffer), 0, 0);
    window.set_size(1100, 700);
    window.ack_last_and_commit();
    f.double_roundtrip(client);
}

fn setup_many(scale: f64, windows: usize) -> Fixture {
    let mut config = Config {
        workspaces: ["first"]
            .into_iter()
            .map(|name| Workspace {
                name: WorkspaceName(name.into()),
                open_on_output: None,
                layout: None,
            })
            .collect(),
        ..Default::default()
    };
    config.hotkey_overlay.skip_at_startup = true;
    config.overview.stage_manager.on = true;
    config.layout.auto_stack_new_windows = false;

    let mut f = Fixture::with_config(config);
    f.init_gles_renderer();
    f.add_output(1, (1280, 800));
    let output = f.clarion_output(1);
    output.change_current_state(None, None, Some(smithay::output::Scale::Fractional(scale)), None);
    f.clarion().output_resized(&output);
    let client = f.add_client();
    for index in 0..windows {
        // Distinct colors per window: red, blue, green, ...
        let color = match index {
            0 => [u32::MAX, 0, 0, u32::MAX],
            1 => [0, 0, u32::MAX, u32::MAX],
            2 => [0, u32::MAX, 0, u32::MAX],
            3 => [u32::MAX, u32::MAX, 0, u32::MAX],
            _ => [u32::MAX, 0, u32::MAX, u32::MAX],
        };
        add_window(&mut f, client, color);
    }
    f.clarion().update_render_elements(None);
    f.double_roundtrip(client);
    f.clarion().update_render_elements(None);
    f.double_roundtrip(client);
    f.clarion().update_render_elements(None);
    f.clarion_complete_animations();
    f
}

fn setup(scale: f64) -> Fixture {
    let mut config = Config {
        workspaces: ["first"]
            .into_iter()
            .map(|name| Workspace {
                name: WorkspaceName(name.into()),
                open_on_output: None,
                layout: None,
            })
            .collect(),
        ..Default::default()
    };
    config.hotkey_overlay.skip_at_startup = true;
    config.overview.stage_manager.on = true;

    let mut f = Fixture::with_config(config);
    f.init_gles_renderer();
    f.add_output(1, (1280, 800));
    let output = f.clarion_output(1);
    output.change_current_state(None, None, Some(smithay::output::Scale::Fractional(scale)), None);
    f.clarion().output_resized(&output);
    let client = f.add_client();
    // Two stages: the newest window owns the stage, the older one waits in the
    // rail.
    add_window(&mut f, client, [0, 0, u32::MAX, u32::MAX]);
    add_window(&mut f, client, [u32::MAX, 0, 0, u32::MAX]);
    f.clarion().update_render_elements(None);
    f.clarion_complete_animations();
    f
}

fn at(image: &[u8], width: usize, point: Point<f64, Logical>, scale: f64) -> [u8; 4] {
    let point = Point::<f64, Physical>::from((point.x * scale, point.y * scale))
        .to_i32_round::<i32>();
    let offset = (point.y as usize * width + point.x as usize) * 4;
    [
        image[offset],
        image[offset + 1],
        image[offset + 2],
        image[offset + 3],
    ]
}

#[test]
fn stage_manager_shows_one_window_at_real_size_and_reveals_the_rail() {
    let scale = 1.;
    let mut f = setup(scale);
    let output = f.clarion_output(1);
    let width = f
        .clarion_output(1)
        .current_mode()
        .unwrap()
        .size
        .w as usize;
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();

    // The focused window is the stage; nothing is scaled or remapped.
    let image = pixels(&mut f, RenderTarget::Output);
    let active_center = {
        let workspace = f
            .clarion()
            .layout
            .monitor_for_output(&output)
            .unwrap()
            .active_workspace_ref();
        let active = workspace.active_window().unwrap().core_id();
        let (tile, pos, _) = workspace
            .tiles_with_render_positions()
            .find(|(tile, _, _)| tile.window().core_id() == active)
            .unwrap();
        pos + tile.tile_size().to_point().downscale(2.)
    };
    let stage = at(&image, width, active_center, scale);
    assert!(stage[0] > 200 && stage[1] < 40 && stage[2] < 40,
        "the focused window owns the stage: {stage:?}");

    // The rail geometry is what the pointer code uses for hit testing.
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    assert!(plan.is_empty(), "a resting pointer keeps the rail closed");

    // Bring the pointer to the left edge: the rail slides out with the parked
    // window's live content.
    f.clarion().clock.set_complete_instantly(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    f.clarion().clock.set_complete_instantly(false);
    assert_eq!(plan.cards.len(), 1, "one parked window waits in the rail");
    let card = plan.cards[0];
    assert!(card.geometry.loc.x >= 0. && card.geometry.loc.x < 60.);
    assert!(
        (card.angle - (-0.62)).abs() < 0.05,
        "cards rest turned into the screen: {}",
        card.angle
    );

    let image = pixels(&mut f, RenderTarget::Output);
    let thumb = at(
        &image,
        width,
        card.geometry.loc + card.geometry.size.to_point().downscale(2.),
        scale,
    );
    assert!(thumb[2] > 200 && thumb[0] < 40,
        "the rail renders the parked window: {thumb:?}");

    // The stage window is untouched by the rail: it still covers the right
    // side, and window hit testing needs no scale mapping.
    let stage_again = at(&image, width, active_center, scale);
    assert_eq!(stage_again, stage);
    let hit = f.clarion().layout.window_under(&output, active_center);
    assert!(hit.is_some(), "the stage window still receives pointer hits");

    // Clicking the card is the rail's job; the card is outside the stage.
    assert_eq!(
        f.clarion()
            .stage_manager_ui
            .card_at(&output, card.geometry.loc + card.geometry.size.to_point().downscale(2.)),
        Some(card.core_id)
    );
}

#[test]
fn stage_manager_click_puts_the_selected_window_on_stage() {
    let scale = 1.;
    let mut f = setup(scale);
    let output = f.clarion_output(1);
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();
    let before = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .active_workspace_ref()
        .active_window()
        .unwrap()
        .core_id();

    // Reveal the rail so its cards become clickable.
    f.clarion().clock.set_complete_instantly(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    f.clarion().clock.set_complete_instantly(false);
    assert_eq!(plan.cards.len(), 1);
    let card = plan.cards[0];
    assert_ne!(card.core_id, before, "the rail holds the other window");
    let center = card.geometry.loc + card.geometry.size.to_point().downscale(2.);

    assert!(f.clarion().stage_manager_click(&output, center));
    assert!(f.clarion().stage_swap_output.is_some(), "the swap animates");
    // The next frame reconciles the stage collection and finishes the swap.
    f.clarion().update_render_elements(None);
    f.clarion_complete_animations();

    let after = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .active_workspace_ref()
        .active_window()
        .unwrap()
        .core_id();
    assert_eq!(after, card.core_id, "the clicked window takes the stage");

    // Nothing is minimized: the old stage window simply returns to the rail.
    let parked: Vec<_> = f
        .clarion()
        .layout
        .stage_manager_windows(&output)
        .iter()
        .map(|window| window.core_id())
        .collect();
    assert!(parked.contains(&before));
    let before_window = f
        .clarion()
        .layout
        .windows()
        .find_map(|(_, window)| (window.core_id() == before).then(|| window.window.clone()))
        .unwrap();
    assert!(!f.clarion().layout.is_minimized(&before_window));
}

#[test]
fn stage_manager_lists_every_collected_window_in_the_rail() {
    let scale = 1.;
    let mut f = setup_many(scale, 5);
    let output = f.clarion_output(1);
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();
    let parked = f.clarion().layout.stage_manager_windows(&output).len();
    assert_eq!(parked, 4, "every other window is collected");

    f.clarion().clock.set_complete_instantly(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    f.clarion().clock.set_complete_instantly(false);
    assert_eq!(plan.cards.len(), 4, "the rail lists every collected window");

    let width = f.clarion_output(1).current_mode().unwrap().size.w as usize;
    let image = pixels(&mut f, RenderTarget::Output);
    let mut seen = Vec::new();
    for card in &plan.cards {
        let center = card.geometry.loc + card.geometry.size.to_point().downscale(2.);
        let pixel = at(&image, width, center, scale);
        seen.push((card.core_id, [pixel[0], pixel[1], pixel[2]]));
        assert!(
            pixel[..3].iter().any(|c| *c > 180),
            "card {} renders its window content: {pixel:?}",
            card.core_id
        );
    }
    let mut unique: Vec<_> = seen.iter().map(|(_, color)| *color).collect();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 4, "each card shows a different window: {seen:?}");
}

#[test]
fn stage_manager_swap_animates_the_outgoing_window_into_the_rail() {
    let scale = 1.;
    let mut f = setup(scale);
    let output = f.clarion_output(1);
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();
    let outgoing = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .active_workspace_ref()
        .active_window()
        .unwrap()
        .core_id();

    f.clarion().clock.set_complete_instantly(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    f.clarion().clock.set_complete_instantly(false);
    let card = plan.cards[0];
    let center = card.geometry.loc + card.geometry.size.to_point().downscale(2.);
    assert!(f.clarion().stage_manager_click(&output, center));
    assert!(f.clarion().stage_swap_output.is_some(), "the flight starts");

    let flight = |f: &mut Fixture, millis: u64| {
        let now = f.clarion().clock.now_unadjusted();
        f.clarion()
            .clock
            .set_unadjusted(now + std::time::Duration::from_millis(millis));
        f.clarion().update_render_elements(None);
        f.clarion()
            .scene_for_output(&output)
            .ui
            .stage_rail
            .cards
            .iter()
            .find(|card| card.core_id == outgoing)
            .map(|card| (card.geometry.size.w, card.geometry.size.h))
    };

    let start = flight(&mut f, 1).expect("the outgoing window flies");
    let middle = flight(&mut f, 110).expect("the outgoing window keeps flying");
    assert!(
        middle.0 < start.0,
        "the outgoing window shrinks: {} -> {}",
        start.0,
        middle.0
    );

    let _ = flight(&mut f, 600);
    assert!(
        f.clarion().stage_swap_output.is_none(),
        "the flight finishes and the outgoing window is shown again"
    );
    let end = f
        .clarion()
        .scene_for_output(&output)
        .ui
        .stage_rail
        .cards;
    assert_eq!(end.len(), 1, "only the collected window remains in the rail");
}

#[test]
fn stage_manager_swap_keeps_the_other_collected_windows_hidden() {
    let scale = 1.;
    let mut f = setup_many(scale, 4);
    let output = f.clarion_output(1);
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();
    f.clarion().clock.set_complete_instantly(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    f.clarion().clock.set_complete_instantly(false);
    let card = plan.cards[0];
    let center = card.geometry.loc + card.geometry.size.to_point().downscale(2.);
    assert!(f.clarion().stage_manager_click(&output, center));
    f.clarion().update_render_elements(None);

    let hidden: Vec<u64> = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .stage_hidden()
        .to_vec();
    assert_eq!(
        hidden.len(),
        4,
        "during the exchange every window is hidden, including the other collected ones: {hidden:?}"
    );

    f.clarion_complete_animations();
    f.clarion().update_render_elements(None);
    let hidden = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .stage_hidden()
        .len();
    assert_eq!(hidden, 3, "afterwards only the collected windows stay hidden");
}

#[test]
fn an_overflowing_rail_scrolls_and_fades_its_edges() {
    let scale = 1.;
    let mut f = setup_many(scale, 9);
    let output = f.clarion_output(1);
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();
    f.clarion().clock.set_complete_instantly(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    f.clarion().clock.set_complete_instantly(false);
    assert_eq!(
        f.clarion().layout.stage_manager_windows(&output).len(),
        8,
        "nine windows leave eight in the rail"
    );
    assert!(
        plan.cards.len() < 8,
        "the ring only draws the copies that intersect the viewport"
    );

    // Pick a card near the middle: it stays visible after one scroll step.
    let center = view.h / 2.;
    let middle = plan
        .cards
        .iter()
        .min_by(|a, b| {
            let a = (a.geometry.loc.y + a.geometry.size.h / 2. - center).abs();
            let b = (b.geometry.loc.y + b.geometry.size.h / 2. - center).abs();
            a.total_cmp(&b)
        })
        .copied()
        .unwrap();
    let step = f.clarion().stage_manager_ui.scroll_step(&output);
    assert!(step > 0., "the overflowing rail has a scroll step");

    // Animations off: the step snaps so the geometry is readable immediately.
    f.clarion().stage_manager_ui.update_config(true);
    assert!(
        f.clarion()
            .stage_manager_ui
            .scroll_by(&output, step * 2.),
        "the overflowing rail accepts the scroll"
    );
    f.clarion().update_render_elements(None);
    let scrolled = f.clarion().scene_for_output(&output).ui.stage_rail;
    assert!(
        scrolled.cards.iter().any(|card| card.alpha < 0.99),
        "cards crossing the 70% viewport edge fade out"
    );
    let moved = scrolled
        .cards
        .iter()
        .find(|card| card.core_id == middle.core_id)
        .expect("the middle card is still in the viewport");
    assert!(
        (moved.geometry.loc.y - (middle.geometry.loc.y - step * 2.)).abs() < 0.5,
        "the strip moves by the scroll delta: {} -> {}",
        middle.geometry.loc.y,
        moved.geometry.loc.y
    );

    // One full period comes back to the same ring position, so the wheel
    // wraps instead of stopping at either end.
    let period = middle.geometry.size.h * 8. + 16. * 8.;
    f.clarion()
        .stage_manager_ui
        .scroll_by(&output, period);
    f.clarion().update_render_elements(None);
    let wrapped = f.clarion().scene_for_output(&output).ui.stage_rail;
    let again = wrapped
        .cards
        .iter()
        .find(|card| card.core_id == middle.core_id)
        .expect("the middle card is still in the viewport");
    assert!(
        (again.geometry.loc.y - moved.geometry.loc.y).abs() < 0.5,
        "a full period returns the same position: {} vs {}",
        moved.geometry.loc.y,
        again.geometry.loc.y
    );
}

#[test]
fn hovering_a_card_only_turns_it_partway() {
    let mut f = setup(1.);
    let output = f.clarion_output(1);
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();
    f.clarion().clock.set_complete_instantly(true);
    f.clarion().stage_manager_ui.update_config(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    f.clarion().update_render_elements(None);
    let plan = f.clarion().scene_for_output(&output).ui.stage_rail;
    let card = plan.cards[0];
    let center = card.geometry.loc + card.geometry.size.to_point().downscale(2.);

    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, center);
    f.clarion().update_render_elements(None);
    let hovered = f
        .clarion()
        .scene_for_output(&output)
        .ui
        .stage_rail
        .cards
        .iter()
        .find(|card| card.core_id == plan.cards[0].core_id)
        .copied()
        .unwrap();
    assert!(
        hovered.angle < -0.3 && hovered.angle > -0.45,
        "hover turns the card partway, never flat: {}",
        hovered.angle
    );
    assert!(
        (hovered.angle - (-0.341)).abs() < 0.02,
        "hover keeps the strip's angle as a cue: {}",
        hovered.angle
    );
    f.clarion().clock.set_complete_instantly(false);
}

#[test]
fn leaving_a_card_animates_back_instead_of_snapping() {
    let mut f = setup(1.);
    let output = f.clarion_output(1);
    let view = f
        .clarion()
        .layout
        .monitor_for_output(&output)
        .unwrap()
        .view_size();
    f.clarion().clock.set_complete_instantly(true);
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., view.h / 2.)));
    f.clarion().update_render_elements(None);
    let resting = f.clarion().scene_for_output(&output).ui.stage_rail.cards[0];
    let center = resting.geometry.loc + resting.geometry.size.to_point().downscale(2.);
    f.clarion().clock.set_complete_instantly(false);

    // Hover: the card lifts smoothly instead of growing in one frame.
    f.clarion().stage_manager_ui.pointer_moved(&output, center);
    let now = f.clarion().clock.now_unadjusted();
    f.clarion()
        .clock
        .set_unadjusted(now + std::time::Duration::from_millis(70));
    f.clarion().update_render_elements(None);
    let hovered = f
        .clarion()
        .scene_for_output(&output)
        .ui
        .stage_rail
        .cards
        .iter()
        .find(|card| card.core_id == resting.core_id)
        .copied()
        .unwrap();
    assert!(
        hovered.geometry.size.w > resting.geometry.size.w,
        "hover lifts the card: {} vs {}",
        hovered.geometry.size.w,
        resting.geometry.size.w
    );

    // Leave the card but keep the rail open: it must ease back, not snap.
    f.clarion()
        .stage_manager_ui
        .pointer_moved(&output, Point::from((2., 4.)));
    let now = f.clarion().clock.now_unadjusted();
    f.clarion()
        .clock
        .set_unadjusted(now + std::time::Duration::from_millis(40));
    f.clarion().update_render_elements(None);
    let leaving = f
        .clarion()
        .scene_for_output(&output)
        .ui
        .stage_rail
        .cards
        .iter()
        .find(|card| card.core_id == resting.core_id)
        .copied()
        .unwrap();
    assert!(
        leaving.geometry.size.w > resting.geometry.size.w,
        "the leave animation is still returning: {} vs {}",
        leaving.geometry.size.w,
        resting.geometry.size.w
    );
    assert!(
        leaving.angle < hovered.angle && leaving.angle > -0.62,
        "the angle returns gradually: {} after {}",
        leaving.angle,
        hovered.angle
    );
}
