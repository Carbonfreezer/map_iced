//! The soft focus: an animated move of a widget's focal point to a new position.
//!
//! The path is chosen by distance, measured per axis in pixels at the current zoom
//! level. A target closer than one widget width (height) is reached by a straight
//! pan. Anything further away first zooms out until the target sits inside the
//! frame, pans there and zooms back in to the zoom level the animation started at.
//! A long pan at high zoom would sweep thousands of tiles nobody can load in time,
//! zooming out keeps the overlap between consecutive frames high.

use crate::gui_system::internal_math::{MAXIMUM_ZOOM_LEVEL, TILE_SIZE_PIXEL, TileCoordinates};
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget_components::FocalPoint;
use iced::Size;
use iced::time::{Duration, Instant};

/// How long a pan takes, independent of its length.
const PAN_DURATION: Duration = Duration::from_millis(1000);

/// How long it takes to zoom by one level.
const ZOOM_DURATION_PER_LEVEL: Duration = Duration::from_millis(150);

/// How many zoom levels we go out beyond the point where the target just touches
/// the edge of the frame, so that it does not sit exactly on that edge.
const FIT_MARGIN: f32 = 0.25;

/// One straight piece of the path. Position and zoom are interpolated
/// independently, so a pan keeps the zoom and a zoom keeps the position.
#[derive(Debug, Clone, Copy)]
struct Segment {
    from: FocalPoint,
    to: FocalPoint,
    duration: Duration,
}

impl Segment {
    /// The focal point at `fraction` (0..=1) of this segment.
    fn sample(&self, fraction: f64) -> FocalPoint {
        // Ease in and out, so the phases join without a jolt.
        let eased = fraction * fraction * (3.0 - 2.0 * fraction);
        // Interpolated in mercator space, where a straight line is a straight pan.
        let from = self.from.position.get_tile_coordinates(0);
        let to = self.to.position.get_tile_coordinates(0);
        let position = TileCoordinates {
            x: from.x + (to.x - from.x) * eased,
            y: from.y + (to.y - from.y) * eased,
            zoom: 0,
        }
        .into();
        let zoom = self.from.continuous_zoom_level as f64
            + (self.to.continuous_zoom_level - self.from.continuous_zoom_level) as f64 * eased;
        FocalPoint {
            position,
            continuous_zoom_level: zoom as f32,
        }
    }
}

/// A running soft focus. It holds no reference to the widget, the frames are
/// computed from the clock alone.
#[derive(Debug, Clone)]
pub(crate) struct FocusAnimation {
    start: Instant,
    segments: Vec<Segment>,
    target: FocalPoint,
}

impl FocusAnimation {
    /// Plans the path from `from` to `target` for a widget of `view_size` pixels.
    /// The animation ends at the zoom level of `from`.
    pub(crate) fn new(
        from: FocalPoint,
        target: LatitudeLongitude,
        view_size: Size,
        start: Instant,
    ) -> Self {
        let target = FocalPoint {
            position: target,
            continuous_zoom_level: from.continuous_zoom_level,
        };
        let segments = match zoom_out_level(&from, &target.position, view_size) {
            None => vec![Segment {
                from,
                to: target,
                duration: PAN_DURATION,
            }],
            Some(zoom_out) => {
                let zoom_duration =
                    ZOOM_DURATION_PER_LEVEL.mul_f32(from.continuous_zoom_level - zoom_out);
                let out = FocalPoint {
                    continuous_zoom_level: zoom_out,
                    ..from
                };
                let over = FocalPoint {
                    continuous_zoom_level: zoom_out,
                    ..target
                };
                vec![
                    Segment {
                        from,
                        to: out,
                        duration: zoom_duration,
                    },
                    Segment {
                        from: out,
                        to: over,
                        duration: PAN_DURATION,
                    },
                    Segment {
                        from: over,
                        to: target,
                        duration: zoom_duration,
                    },
                ]
            }
        };
        Self {
            start,
            segments,
            target,
        }
    }

    /// The focal point at `now`, and whether the animation is over with it.
    pub(crate) fn sample(&self, now: Instant) -> (FocalPoint, bool) {
        let mut elapsed = now.saturating_duration_since(self.start);
        for segment in &self.segments {
            if elapsed < segment.duration {
                let fraction = elapsed.as_secs_f64() / segment.duration.as_secs_f64();
                return (segment.sample(fraction), false);
            }
            elapsed -= segment.duration;
        }
        (self.target, true)
    }
}

/// The pixel offset from `from` to `target` at the zoom level of `from`, not
/// clipped against anything.
fn pixel_offset(from: &FocalPoint, target: &LatitudeLongitude) -> (f64, f64) {
    let scale = TILE_SIZE_PIXEL as f64 * f64::exp2(from.continuous_zoom_level as f64);
    let a = from.position.get_tile_coordinates(0);
    let b = target.get_tile_coordinates(0);
    ((b.x - a.x) * scale, (b.y - a.y) * scale)
}

/// The zoom level to go out to before panning, or `None` if a direct pan does.
///
/// A direct pan is fine while the target is no further than one width or height
/// away: that is the last distance at which the start and the end view still share
/// an edge. Beyond it we zoom out, with the centre staying put, until the target is
/// half a width or height from the centre and thereby inside the frame. Each axis
/// gives its own level, the smaller one is binding.
fn zoom_out_level(from: &FocalPoint, target: &LatitudeLongitude, view_size: Size) -> Option<f32> {
    let (dx, dy) = pixel_offset(from, target);
    let (width, height) = (view_size.width as f64, view_size.height as f64);
    if dx.abs() <= width && dy.abs() <= height {
        return None;
    }

    let level = |distance: f64, half_extent: f64| {
        if distance <= half_extent {
            f64::INFINITY
        } else {
            from.continuous_zoom_level as f64 - f64::log2(distance / half_extent)
        }
    };
    let zoom = level(dx.abs(), width * 0.5).min(level(dy.abs(), height * 0.5)) as f32 - FIT_MARGIN;
    Some(zoom.clamp(0.0, MAXIMUM_ZOOM_LEVEL as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start view of the examples, with the viewport measured there.
    fn trier() -> FocalPoint {
        FocalPoint {
            position: LatitudeLongitude::new(49.75, 6.63),
            continuous_zoom_level: 12.0,
        }
    }

    const VIEW: Size = Size {
        width: 594.0,
        height: 492.0,
    };

    #[test]
    fn mertert_is_a_pan() {
        let mertert = LatitudeLongitude::new(49.7031, 6.4797);
        let (dx, _) = pixel_offset(&trier(), &mertert);
        // Off screen, but within one width.
        assert!(dx.abs() > VIEW.width as f64 * 0.5);
        assert!(zoom_out_level(&trier(), &mertert, VIEW).is_none());
    }

    #[test]
    fn montreal_zooms_out_far() {
        let montreal = LatitudeLongitude::new(45.5017, -73.5673);
        let zoom = zoom_out_level(&trier(), &montreal, VIEW).expect("too far for a pan");
        assert!(zoom < 4.0, "zoom out level {zoom}");

        // At that level the target sits inside the frame.
        let out = FocalPoint {
            continuous_zoom_level: zoom,
            ..trier()
        };
        let (dx, dy) = pixel_offset(&out, &montreal);
        assert!(dx.abs() < VIEW.width as f64 * 0.5);
        assert!(dy.abs() < VIEW.height as f64 * 0.5);
    }

    #[test]
    fn animation_ends_on_target_at_start_zoom() {
        let koeln = LatitudeLongitude::new(50.9375, 6.9603);
        let start = Instant::now();
        let animation = FocusAnimation::new(trier(), koeln, VIEW, start);
        assert_eq!(animation.segments.len(), 3);

        let (begin, finished) = animation.sample(start);
        assert!(!finished);
        assert!((begin.position.latitude - 49.75).abs() < 1e-9);

        let (end, finished) = animation.sample(start + Duration::from_secs(60));
        assert!(finished);
        assert!((end.position.latitude - koeln.latitude).abs() < 1e-9);
        assert!((end.position.longitude - koeln.longitude).abs() < 1e-9);
        assert_eq!(end.continuous_zoom_level, 12.0);
    }
}
