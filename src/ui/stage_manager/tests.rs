use smithay::utils::{Logical, Rectangle, Size};

use super::rail::{rail_geometry, RailGeometry};
use super::*;

fn tuning() -> StageRailTuning {
    StageRailTuning::default()
}

fn area(height: f64) -> Rectangle<f64, Logical> {
    Rectangle::from_size(Size::from((1280., height)))
}

fn size(w: f64, h: f64) -> Size<f64, Logical> {
    Size::from((w, h))
}

#[test]
fn cards_keep_the_window_aspect_at_the_shared_scale() {
    let windows = vec![
        (1, size(1920., 1080.)),
        (2, size(700., 1000.)),
        (3, size(1000., 700.)),
    ];
    let geometry = rail_geometry(area(800.), 1., &windows, 0., &tuning());
    assert!(!geometry.scrolling);
    let base = 800. * tuning().height_fraction;
    for (id, window) in &windows {
        let card = geometry
            .cards
            .iter()
            .find(|(card_id, _, _)| card_id == id)
            .map(|(_, rect, _)| *rect)
            .unwrap();
        assert!(
            (card.size.h - window.h * base / 800.).abs() < 0.01,
            "card {id} keeps the shared scale: {card:?}"
        );
        assert!(
            (card.size.w / card.size.h - window.w / window.h).abs() < 0.001,
            "card {id} keeps the window aspect: {card:?}"
        );
        assert!(card.size.w > 0. && card.size.h > 0.);
    }
}

#[test]
fn a_tall_strip_scrolls_and_fades_at_the_viewport_edges() {
    let windows: Vec<_> = (0..10)
        .map(|id| (id as u64, size(1100., 700.)))
        .collect();
    // Park one card half over the top edge: it must fade.
    let geometry = rail_geometry(area(800.), 1., &windows, -260., &tuning());
    assert!(geometry.scrolling, "ten cards exceed 70% of the height");
    assert!(
        geometry.cards.iter().any(|(_, _, alpha)| *alpha < 0.98),
        "cards crossing the viewport edge fade: {:?}",
        geometry.cards
    );
    let top = 800. / 2. - 800. * tuning().viewport_fraction / 2.;
    let bottom = top + 800. * tuning().viewport_fraction;
    for (_, rect, alpha) in &geometry.cards {
        assert!(*alpha > 0., "only visible copies are kept");
        assert!(
            rect.loc.y < bottom && rect.loc.y + rect.size.h > top,
            "every kept card intersects the viewport: {rect:?}"
        );
    }
}

#[test]
fn scrolling_wraps_after_one_full_period() {
    let windows: Vec<_> = (0..10)
        .map(|id| (id as u64, size(1100., 700.)))
        .collect();
    let start = rail_geometry(area(800.), 1., &windows, 0., &tuning());
    // Heights are uniform, so the period is ten card slots.
    let base = (800. * tuning().height_fraction).clamp(tuning().base_min, tuning().base_max);
    let card_height = 700. * base / 800.;
    let period = card_height * 10. + tuning().gap * 10.;
    let wrapped = rail_geometry(area(800.), 1., &windows, period, &tuning());
    let before: Vec<_> = start
        .cards
        .iter()
        .map(|(id, rect, _)| (*id, rect.loc.y))
        .collect();
    let after: Vec<_> = wrapped
        .cards
        .iter()
        .map(|(id, rect, _)| (*id, rect.loc.y))
        .collect();
    assert_eq!(
        before.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        after.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        "a full period returns the same cards"
    );
    for ((_, left), (_, right)) in before.iter().zip(after.iter()) {
        assert!(
            (left - right).abs() < 0.01,
            "a full period returns the same positions: {left} vs {right}"
        );
    }
}

#[test]
fn an_overflowing_strip_starts_with_the_mru_card_at_the_top() {
    let windows: Vec<_> = (0..10)
        .map(|id| (id as u64, size(1000., 700.)))
        .collect();
    let geometry = rail_geometry(area(800.), 1., &windows, 0., &tuning());
    let top = 800. / 2. - 800. * tuning().viewport_fraction / 2.;
    let first = geometry
        .cards
        .iter()
        .find(|(id, _, _)| *id == 0)
        .expect("the MRU card is visible");
    assert!(
        (first.1.loc.y - top).abs() < 0.01,
        "the MRU card opens the strip: {} vs {top}",
        first.1.loc.y
    );
    assert!(
        geometry.cards.iter().all(|(_, rect, _)| rect.loc.y >= top - 0.5),
        "nothing is drawn above the viewport"
    );
}

#[test]
fn the_reveal_cascades_the_cards_out() {
    let windows: Vec<_> = (0..6)
        .map(|id| (id as u64, size(1000., 700.)))
        .collect();
    let partial = rail_geometry(area(800.), 0.75, &windows, 0., &tuning());
    let first = partial
        .cards
        .iter()
        .find(|(id, _, _)| *id == 0)
        .unwrap();
    let last = partial
        .cards
        .iter()
        .find(|(id, _, _)| *id == 5)
        .unwrap();
    assert!(
        first.2 > last.2,
        "the top card leads the reveal: {} vs {}",
        first.2,
        last.2
    );
    assert!(last.2 < 0.5, "the last card is still fading in: {}", last.2);
    assert!(
        first.1.loc.x > last.1.loc.x,
        "the top card is further out than the last: {} vs {}",
        first.1.loc.x,
        last.1.loc.x
    );
    let full = rail_geometry(area(800.), 1., &windows, 0., &tuning());
    assert_eq!(full.cards.len(), 6);
    assert!(full.cards.iter().all(|(_, _, alpha)| *alpha > 0.99));
}

#[test]
fn the_ring_keeps_the_same_gap_across_the_seam() {
    let windows: Vec<_> = (0..10)
        .map(|id| (id as u64, size(1000., 700.)))
        .collect();
    // Scroll up so the wrapped tail is entering just above the first card.
    let geometry = rail_geometry(area(800.), 1., &windows, -40., &tuning());
    let first = geometry
        .cards
        .iter()
        .find(|(id, _, _)| *id == 0)
        .unwrap();
    let last = geometry
        .cards
        .iter()
        .find(|(id, _, _)| *id == 9)
        .unwrap();
    let seam = first.1.loc.y - (last.1.loc.y + last.1.size.h);
    assert!(
        (seam - tuning().gap).abs() < 0.01,
        "head and tail keep the card gap: {seam}"
    );
}

#[test]
fn a_short_strip_stays_centred_and_opaque() {
    let windows = vec![(1, size(1000., 700.)), (2, size(1000., 700.))];
    let geometry = rail_geometry(area(800.), 1., &windows, 0., &tuning());
    assert!(!geometry.scrolling);
    assert_eq!(geometry.cards.len(), 2);
    assert_eq!(geometry.cards[0].0, 1, "MRU stays on top");
    assert!(
        geometry.cards[0].1.loc.y < geometry.cards[1].1.loc.y,
        "the short strip keeps its order"
    );
    let card = geometry.cards[0].1;
    assert!(card.loc.y > 0., "cards sit inside the working area");
    assert!(geometry.cards.iter().all(|(_, _, alpha)| *alpha == 1.));
    let top = geometry.cards.first().unwrap().1.loc.y;
    let bottom = geometry.cards.last().unwrap().1.loc.y
        + geometry.cards.last().unwrap().1.size.h;
    assert!(
        (top + bottom - 800.).abs() < 0.01,
        "the short strip is vertically centred"
    );
}

#[test]
fn a_custom_tuning_changes_the_rail_without_touching_the_state_machine() {
    let windows = vec![(1, size(1000., 800.)), (2, size(1000., 800.))];
    let default = rail_geometry(area(800.), 1., &windows, 0., &tuning());
    let custom = StageRailTuning {
        gap: 40.,
        height_fraction: 0.2,
        ..tuning()
    };
    let tuned = rail_geometry(area(800.), 1., &windows, 0., &custom);

    let default_card = default.cards[0].1;
    let tuned_card = tuned.cards[0].1;
    assert!((default_card.size.h - 80.).abs() < 0.01);
    assert!((tuned_card.size.h - 160.).abs() < 0.01);

    let gap = |geometry: &RailGeometry| {
        geometry.cards[1].1.loc.y
            - (geometry.cards[0].1.loc.y + geometry.cards[0].1.size.h)
    };
    assert!((gap(&default) - 16.).abs() < 0.01);
    assert!((gap(&tuned) - 40.).abs() < 0.01);
}
