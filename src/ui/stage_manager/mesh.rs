//! Pure mesh projection for a rail card.

use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::render_helpers::mesh::MeshVertex;

use super::StageRailCard;

/// Project a card around the vertical axis with the affine shear that makes
/// the whole strip read as one rotated plane.
///
/// The card's right edge recedes for negative angles. Returns the projected
/// bounding area (local, starting at zero) and the triangle strip vertices.
fn card_mesh(
    size: Size<f64, Logical>,
    angle: f64,
    shear: f64,
) -> Option<(Rectangle<f64, Logical>, Vec<MeshVertex>)> {
    if size.w <= 0. || size.h <= 0. || angle.cos().abs() < 0.00001 {
        return None;
    }
    // Affine, shared by every card: horizontal compression plus a vertical
    // shear. No per-card perspective, so all card edges stay parallel.
    let project = |u: f64, v: f64| {
        let x = u * size.w;
        let y = v * size.h;
        Point::from((x * angle.cos(), y + x * angle.sin() * shear))
    };
    let quad = [
        project(0., 0.),
        project(1., 0.),
        project(1., 1.),
        project(0., 1.),
    ];
    let min = Point::from((
        quad.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        quad.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
    ));
    let max = Point::from((
        quad.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
        quad.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
    ));
    let area = Rectangle::from_extremities(min, max);
    if area.size.w <= 0.01 || area.size.h <= 0.01 {
        return None;
    }
    let normalize = |point: Point<f64, Logical>| {
        [
            ((point.x - area.loc.x) / area.size.w) as f32,
            ((point.y - area.loc.y) / area.size.h) as f32,
        ]
    };
    let uvs = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
    let order = [0usize, 1, 2, 0, 2, 3];
    let vertices = order
        .iter()
        .map(|index| MeshVertex {
            position: normalize(quad[*index]),
            uv: uvs[*index],
        })
        .collect();
    Some((
        Rectangle::new(Point::from((area.loc.x, area.loc.y)), area.size),
        vertices,
    ))
}

pub(super) fn card_mesh_area(
    card: &StageRailCard,
    shear: f64,
) -> Option<(Rectangle<f64, Logical>, Vec<MeshVertex>)> {
    let (local, vertices) = card_mesh(card.geometry.size, card.angle, shear)?;
    // Hinge on the left edge: that edge keeps its slot position.
    Some((
        Rectangle::new(card.geometry.loc + local.loc, local.size),
        vertices,
    ))
}

