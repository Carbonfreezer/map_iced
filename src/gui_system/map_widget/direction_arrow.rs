//! The direction arrow of a flagged way point that is off screen, drawn like on a
//! radar: the tip sits on the edge of the widget and points towards the way point.
//!
//! During a soft focus an arrow does not vanish when its way point comes into view,
//! it attaches to the symbol instead and stays until the animation is over.

use crate::annotation_system::waypoint_system::WaypointKey;
use iced::advanced::graphics::geometry::Frame;
use iced::widget::canvas::{Path, Stroke, stroke};
use iced::{Color, Point, Renderer, Size, Vector};

/// How far the tip stays inside the edge, so that it is not cut off.
const EDGE_INSET: f32 = 2.0;

/// The gap between the tip of an attached arrow and the symbol it points at.
const SYMBOL_GAP: f32 = 2.0;

/// Length of the arrow from tip to base.
const ARROW_LENGTH: f32 = 18.0;

/// Half the width of the arrow base.
const ARROW_HALF_WIDTH: f32 = 8.0;

/// How far around the arrow a click still counts as a hit.
const HIT_TOLERANCE: f32 = 3.0;

/// The outline that keeps the arrow visible on a map of the same colour.
const OUTLINE_COLOR: Color = Color::from_rgb(0.1, 0.1, 0.1);

/// Where the arrow sits: the tip, and the unit vector it points along.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ArrowPlacement {
    pub tip: Point,
    pub direction: Vector,
}

/// An arrow as drawn in one frame. Kept until the next one, because an attached
/// arrow carries its direction over from the frame before.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlacedArrow {
    pub key: WaypointKey,
    pub placement: ArrowPlacement,
    pub color: Color,
}

/// Whether the symbol of half size `half_size` at `target`, given unclipped in
/// widget coordinates, is at least partly on screen.
pub(crate) fn symbol_visible(size: Size, target: Vector<f64>, half_size: f64) -> bool {
    (-half_size..=size.width as f64 + half_size).contains(&target.x)
        && (-half_size..=size.height as f64 + half_size).contains(&target.y)
}

/// The radar arrow for an off screen way point at `target`. The tip is where the ray
/// from the widget centre to the target leaves the frame, pulled in by
/// [`EDGE_INSET`]. `None` if the widget is too small to hold an arrow.
pub(crate) fn radar_arrow(size: Size, target: Vector<f64>) -> Option<ArrowPlacement> {
    let (half_width, half_height) = (size.width as f64 * 0.5, size.height as f64 * 0.5);
    let (reach_x, reach_y) = (
        half_width - EDGE_INSET as f64,
        half_height - EDGE_INSET as f64,
    );
    if reach_x <= 0.0 || reach_y <= 0.0 {
        return None;
    }

    let (dx, dy) = (target.x - half_width, target.y - half_height);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return None;
    }
    let scale = (reach_x / dx.abs()).min(reach_y / dy.abs());
    Some(ArrowPlacement {
        tip: Point::new(
            (half_width + dx * scale) as f32,
            (half_height + dy * scale) as f32,
        ),
        direction: Vector::new((dx / length) as f32, (dy / length) as f32),
    })
}

/// The arrow attached to a visible symbol at `target`, pointing at it along
/// `direction`, with the tip just in front of the symbol.
pub(crate) fn attached_arrow(
    target: Vector<f64>,
    direction: Vector,
    half_size: f32,
) -> ArrowPlacement {
    let centre = Point::new(target.x as f32, target.y as f32);
    ArrowPlacement {
        tip: centre - direction * (half_size + SYMBOL_GAP),
        direction,
    }
}

/// Whether `position` hits the arrow. The triangle is grown by [`HIT_TOLERANCE`] on
/// every side, it is small enough that an exact test would make it fiddly to click.
pub(crate) fn arrow_hit(placement: ArrowPlacement, position: Point) -> bool {
    let ArrowPlacement { tip, direction } = placement;
    let offset = position - tip;
    // Distance from the tip back towards the base, and sideways from the axis.
    let along = -(offset.x * direction.x + offset.y * direction.y);
    let across = (offset.x * -direction.y + offset.y * direction.x).abs();
    (-HIT_TOLERANCE..=ARROW_LENGTH + HIT_TOLERANCE).contains(&along)
        && across
            <= ARROW_HALF_WIDTH * along.clamp(0.0, ARROW_LENGTH) / ARROW_LENGTH + HIT_TOLERANCE
}

/// Draws the arrow as a filled triangle with an outline.
pub(crate) fn draw_arrow(frame: &mut Frame<Renderer>, placement: ArrowPlacement, color: Color) {
    let ArrowPlacement { tip, direction } = placement;
    let base = tip - direction * ARROW_LENGTH;
    let across = Vector::new(-direction.y, direction.x) * ARROW_HALF_WIDTH;

    let triangle = Path::new(|builder| {
        builder.move_to(tip);
        builder.line_to(base + across);
        builder.line_to(base - across);
        builder.close();
    });
    frame.fill(&triangle, color);
    frame.stroke(
        &triangle,
        Stroke {
            width: 1.5,
            style: stroke::Style::Solid(OUTLINE_COLOR),
            ..Stroke::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size {
        width: 600.0,
        height: 400.0,
    };

    #[test]
    fn visibility_includes_the_symbol_margin() {
        assert!(symbol_visible(SIZE, Vector::new(300.0, 200.0), 10.0));
        // Off by less than the symbol size, part of the symbol still shows.
        assert!(symbol_visible(SIZE, Vector::new(605.0, 200.0), 10.0));
        assert!(!symbol_visible(SIZE, Vector::new(611.0, 200.0), 10.0));
    }

    #[test]
    fn tip_sits_on_the_edge_it_leaves_through() {
        // Straight to the right.
        let arrow = radar_arrow(SIZE, Vector::new(5000.0, 200.0)).unwrap();
        assert_eq!(arrow.tip, Point::new(600.0 - EDGE_INSET, 200.0));
        assert_eq!(arrow.direction, Vector::new(1.0, 0.0));

        // Up and to the left at 45 degrees leaves through the top edge, the frame is
        // wider than high.
        let arrow = radar_arrow(SIZE, Vector::new(-700.0, -800.0)).unwrap();
        assert!((arrow.tip.y - EDGE_INSET).abs() < 1e-3);
        assert!((arrow.tip.x - (300.0 - 200.0 + EDGE_INSET)).abs() < 1e-3);
        assert!(arrow.direction.x < 0.0 && arrow.direction.y < 0.0);
    }

    #[test]
    fn far_point_keeps_its_direction() {
        // Millions of pixels away, as at high zoom on another continent.
        let arrow = radar_arrow(SIZE, Vector::new(-3.0e7, 2.0e6)).unwrap();
        assert!((arrow.tip.x - EDGE_INSET).abs() < 1e-3);
        assert!(arrow.tip.y > 200.0 && arrow.tip.y < 400.0);
    }

    #[test]
    fn hit_test_follows_the_triangle() {
        // Pointing right, tip at (100, 50), base at x = 82.
        let arrow = ArrowPlacement {
            tip: Point::new(100.0, 50.0),
            direction: Vector::new(1.0, 0.0),
        };
        assert!(arrow_hit(arrow, Point::new(90.0, 50.0)));
        // Near the base the triangle is wide, near the tip it is not.
        assert!(arrow_hit(arrow, Point::new(83.0, 57.0)));
        assert!(!arrow_hit(arrow, Point::new(98.0, 57.0)));
        // Within the tolerance in front of the tip, beyond it behind the base.
        assert!(arrow_hit(arrow, Point::new(102.0, 50.0)));
        assert!(!arrow_hit(arrow, Point::new(70.0, 50.0)));
    }

    #[test]
    fn attached_arrow_stops_in_front_of_the_symbol() {
        let arrow = attached_arrow(Vector::new(100.0, 50.0), Vector::new(-1.0, 0.0), 10.0);
        assert_eq!(arrow.tip, Point::new(100.0 + 10.0 + SYMBOL_GAP, 50.0));
        assert_eq!(arrow.direction, Vector::new(-1.0, 0.0));
    }
}
