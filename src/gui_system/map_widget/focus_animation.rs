//! The soft focus: an animated move of a widget's focal point to a new view.
//!
//! The path is chosen by distance, measured per axis in pixels at the lower of the
//! start and the end zoom level. A target closer than one widget width (height) is
//! reached by a straight pan at that lower level, with a zoom before or after it if
//! the levels differ. Anything further away first zooms out until the target sits
//! inside the frame, pans there and zooms in to the target's zoom level. A long pan
//! at high zoom would sweep thousands of tiles nobody can load in time, zooming out
//! keeps the overlap between consecutive frames high.
//!
//! Where the focus ends is set by a [`FocusTarget`], see [`resolve_target`].

use crate::gui_system::internal_math::{MAXIMUM_ZOOM_LEVEL, TILE_SIZE_PIXEL, TileCoordinates};
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::map_widget_components::{FocalPoint, FocusTarget};
use iced::Size;
use iced::time::{Duration, Instant};

/// How long a pan takes, independent of its length.
const PAN_DURATION: Duration = Duration::from_millis(1000);

/// How long it takes to zoom by one level.
const ZOOM_DURATION_PER_LEVEL: Duration = Duration::from_millis(150);

/// How many zoom levels we go out beyond the point where the target just touches
/// the edge of the frame, so that it does not sit exactly on that edge.
const FIT_MARGIN: f32 = 0.25;

/// The closest an area focus zooms in. A tiny region, or one of a single point,
/// would otherwise end up at the maximum zoom level with no surroundings left.
const MAXIMUM_AREA_ZOOM_LEVEL: f32 = 17.0;

/// Below this difference two zoom levels count as the same, no zoom phase needed.
const ZOOM_EPSILON: f32 = 1e-3;

/// Below this distance in pixels a pan is not worth its duration.
const PAN_EPSILON_PIXEL: f64 = 0.5;

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
    ///
    /// The pan always runs at one zoom level, the lower of start and end, or lower
    /// still if the target is too far for a direct pan. A zoom phase before and after
    /// it brings the view there and on to the target's level; phases that would not
    /// change anything are left out.
    pub(crate) fn new(from: FocalPoint, target: FocalPoint, view_size: Size, start: Instant) -> Self {
        let lower = FocalPoint {
            continuous_zoom_level: from
                .continuous_zoom_level
                .min(target.continuous_zoom_level),
            ..from
        };
        let pan_level = zoom_out_level(&lower, &target.position, view_size)
            .unwrap_or(lower.continuous_zoom_level);
        let out = FocalPoint {
            continuous_zoom_level: pan_level,
            ..from
        };
        let over = FocalPoint {
            continuous_zoom_level: pan_level,
            ..target
        };

        let zoom = |from: FocalPoint, to: FocalPoint| {
            let levels = (from.continuous_zoom_level - to.continuous_zoom_level).abs();
            (levels > ZOOM_EPSILON).then(|| Segment {
                from,
                to,
                duration: ZOOM_DURATION_PER_LEVEL.mul_f32(levels),
            })
        };
        let (dx, dy) = pixel_offset(&out, &target.position);
        let pan = (dx.hypot(dy) > PAN_EPSILON_PIXEL).then_some(Segment {
            from: out,
            to: over,
            duration: PAN_DURATION,
        });
        let segments = [zoom(from, out), pan, zoom(over, target)]
            .into_iter()
            .flatten()
            .collect();
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

/// Where a focus on `target` ends, for a widget showing `current` at `view_size`
/// pixels. Without a size, an area is only centred and keeps the zoom level.
///
/// An area is fitted per axis: each axis gives the level at which the area's extent
/// plus its pixel padding is `fill` of the widget's width or height, and the smaller
/// level is binding, so an elongated area fits as well. The centre is taken in
/// mercator space, where it is the visual centre of the area on the map. An area
/// built from nothing has no position, the view stays where it is then.
pub(crate) fn resolve_target(
    target: &FocusTarget,
    current: FocalPoint,
    view_size: Option<Size>,
) -> FocalPoint {
    let (bounds, fill) = match target {
        FocusTarget::Position(position) => {
            return FocalPoint {
                position: *position,
                ..current
            };
        }
        FocusTarget::Area { bounds, .. } if !bounds.is_valid => return current,
        FocusTarget::Area { bounds, fill } => (bounds, *fill as f64),
    };

    let top_left = LatitudeLongitude::new(bounds.min_max_lat.1, bounds.min_max_long.0)
        .get_tile_coordinates(0);
    let bottom_right = LatitudeLongitude::new(bounds.min_max_lat.0, bounds.min_max_long.1)
        .get_tile_coordinates(0);
    let position = TileCoordinates {
        x: (top_left.x + bottom_right.x) * 0.5,
        y: (top_left.y + bottom_right.y) * 0.5,
        zoom: 0,
    }
    .into();
    let Some(view_size) = view_size else {
        return FocalPoint {
            position,
            ..current
        };
    };

    // The extent in pixels at zoom level 0, the padding does not scale with zoom.
    let padding = 2.0 * bounds.pixel_padding;
    let level = |extent: f64, view_extent: f64| {
        let available = (view_extent * fill - padding).max(1.0);
        let extent = extent * TILE_SIZE_PIXEL as f64;
        if extent <= 0.0 {
            f64::INFINITY
        } else {
            f64::log2(available / extent)
        }
    };
    let zoom = level(bottom_right.x - top_left.x, view_size.width as f64)
        .min(level(bottom_right.y - top_left.y, view_size.height as f64));
    FocalPoint {
        position,
        continuous_zoom_level: (zoom as f32).clamp(0.0, MAXIMUM_AREA_ZOOM_LEVEL),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_system::annotation_support::MercatorRectangle;

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
        let target = FocalPoint {
            position: koeln,
            ..trier()
        };
        let animation = FocusAnimation::new(trier(), target, VIEW, start);
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

    /// An area around Trier, `half_lat` and `half_long` degrees to each side.
    fn area(half_lat: f64, half_long: f64, fill: f32) -> FocusTarget {
        FocusTarget::Area {
            bounds: MercatorRectangle {
                min_max_lat: (49.75 - half_lat, 49.75 + half_lat),
                min_max_long: (6.63 - half_long, 6.63 + half_long),
                pixel_padding: 0.0,
                is_valid: true,
            },
            fill,
        }
    }

    /// The extent of `target` in pixels at the zoom level of `view`.
    fn extent_in_pixels(target: &FocusTarget, view: &FocalPoint) -> (f64, f64) {
        let FocusTarget::Area { bounds, .. } = target else {
            panic!("an area is expected");
        };
        let corner = |lat, long| LatitudeLongitude::new(lat, long);
        let at_view = FocalPoint {
            position: corner(bounds.min_max_lat.1, bounds.min_max_long.0),
            ..*view
        };
        let (dx, dy) = pixel_offset(&at_view, &corner(bounds.min_max_lat.0, bounds.min_max_long.1));
        (dx.abs(), dy.abs())
    }

    #[test]
    fn wide_area_fits_the_width() {
        let target = area(0.01, 1.0, 0.8);
        let end = resolve_target(&target, trier(), Some(VIEW));
        let (width, height) = extent_in_pixels(&target, &end);
        assert!((width - VIEW.width as f64 * 0.8).abs() < 0.5, "width {width}");
        assert!(height < VIEW.height as f64 * 0.8);
    }

    #[test]
    fn tall_area_fits_the_height() {
        let target = area(1.0, 0.01, 0.8);
        let end = resolve_target(&target, trier(), Some(VIEW));
        let (width, height) = extent_in_pixels(&target, &end);
        assert!((height - VIEW.height as f64 * 0.8).abs() < 0.5, "height {height}");
        assert!(width < VIEW.width as f64 * 0.8);
        // The centre is the mercator centre, which lies north of the mean latitude.
        assert!(end.position.latitude > 49.75);
    }

    #[test]
    fn tiny_and_invalid_areas() {
        let point = resolve_target(&area(0.0, 0.0, 0.8), trier(), Some(VIEW));
        assert_eq!(point.continuous_zoom_level, MAXIMUM_AREA_ZOOM_LEVEL);

        let invalid = FocusTarget::Area {
            bounds: MercatorRectangle::default(),
            fill: 0.8,
        };
        let unchanged = resolve_target(&invalid, trier(), Some(VIEW));
        assert_eq!(unchanged.continuous_zoom_level, 12.0);
        assert_eq!(unchanged.position.latitude, 49.75);
    }

    #[test]
    fn area_without_size_is_only_centred() {
        let end = resolve_target(&area(1.0, 1.0, 0.8), trier(), None);
        assert_eq!(end.continuous_zoom_level, 12.0);
    }

    #[test]
    fn zoom_in_on_the_spot_has_no_pan() {
        let start = Instant::now();
        let target = FocalPoint {
            continuous_zoom_level: 15.0,
            ..trier()
        };
        let animation = FocusAnimation::new(trier(), target, VIEW, start);
        assert_eq!(animation.segments.len(), 1);
        let (end, finished) = animation.sample(start + Duration::from_secs(60));
        assert!(finished);
        assert_eq!(end.continuous_zoom_level, 15.0);
    }

    #[test]
    fn close_area_pans_then_zooms_in() {
        // Within one width at zoom 12, ending further in: pan first at the lower level.
        let mertert = FocalPoint {
            position: LatitudeLongitude::new(49.7031, 6.4797),
            continuous_zoom_level: 15.0,
        };
        let animation = FocusAnimation::new(trier(), mertert, VIEW, Instant::now());
        assert_eq!(animation.segments.len(), 2);
        assert_eq!(animation.segments[0].to.continuous_zoom_level, 12.0);
        assert_eq!(animation.segments[1].to.continuous_zoom_level, 15.0);
    }

    #[test]
    fn far_area_zooms_out_below_both_levels() {
        let montreal = FocalPoint {
            position: LatitudeLongitude::new(45.5017, -73.5673),
            continuous_zoom_level: 8.0,
        };
        let start = Instant::now();
        let animation = FocusAnimation::new(trier(), montreal, VIEW, start);
        assert_eq!(animation.segments.len(), 3);
        let pan_level = animation.segments[1].from.continuous_zoom_level;
        assert!(pan_level < 8.0, "pan level {pan_level}");
        let (end, finished) = animation.sample(start + Duration::from_secs(60));
        assert!(finished);
        assert_eq!(end.continuous_zoom_level, 8.0);
    }
}
