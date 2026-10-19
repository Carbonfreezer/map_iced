//! Where a widget looks: the view it shows, what a focus aims at, and the focus
//! it still has to publish.
//!
//! A focus is kept as a [`FocusTarget`] until the widget publishes its next view and
//! only then resolved into a [`FocalPoint`], with the bounds of that moment. Before
//! the first layout there are no bounds, and an area can only be fitted with them.

use crate::annotation_system::annotation_support::MercatorRectangle;
use crate::gui_system::internal_math::{TILE_SIZE_PIXEL, TileCoordinates};
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget_system::MapWidgetSystem;
use iced::Size;

/// The closest an area focus zooms in. A tiny region, or one of a single point,
/// would otherwise end up at the maximum zoom level with no surroundings left.
const MAXIMUM_AREA_ZOOM_LEVEL: f32 = 17.0;

/// Focal point info consisting of latitude, longitude and a
/// continuous zoom level.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FocalPoint {
    pub(crate) position: LatitudeLongitude,
    pub(crate) continuous_zoom_level: f32,
}

/// What a focus aims at. Turned into a `FocalPoint` by the widget itself, since
/// only the widget knows its size at that moment.
#[derive(Debug, Clone)]
pub enum FocusTarget {
    /// A single position, the zoom level stays as it is.
    Position(LatitudeLongitude),
    /// An area, typically the bounds of a region. It gets centred and zoomed so that
    /// it covers the share `fill` (0..=1) of the widget's width or height, whichever
    /// is binding.
    Area {
        bounds: MercatorRectangle,
        fill: f32,
    },
}

impl From<LatitudeLongitude> for FocusTarget {
    fn from(position: LatitudeLongitude) -> Self {
        FocusTarget::Position(position)
    }
}

impl FocusTarget {
    /// The area within `radius_meter` around `center`, e.g. 200 m around a vehicle
    /// to keep in view. The circle covers the share
    /// [`MapWidgetSystem::DEFAULT_FOCUS_FILL`] of the widget's shorter side, so a bit
    /// more than the radius stays visible to every side.
    ///
    /// To follow a moving vehicle, call [`MapWidgetSystem::jump_to`] with each new
    /// position. [`MapWidgetSystem::animate_to`] starts its pan and zoom anew on every
    /// call, which suits single moves rather than a stream of them.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::{FocusTarget, MapWidgetSystem};
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let widget = system.request_new_widget();
    /// // The latest position of the car, with 200 m of its surroundings.
    /// let car = LatitudeLongitude::new(49.7499, 6.6371);
    /// system.jump_to(widget, FocusTarget::around(car, 200.0));
    /// ```
    pub fn around(center: LatitudeLongitude, radius_meter: f64) -> Self {
        FocusTarget::Area {
            bounds: MercatorRectangle::create_around(center, radius_meter, 0.0),
            fill: MapWidgetSystem::DEFAULT_FOCUS_FILL,
        }
    }
}

/// A view the widget still has to publish. The canvas publishes it with the next
/// event, since only then are the bounds known.
#[derive(Debug, Clone)]
pub(crate) enum FocusRequest {
    /// The view stays as it is.
    None,
    /// The current view again, e.g. after the annotations changed.
    Republish,
    /// A hard focus, resolved with the bounds it is published with.
    Target(FocusTarget),
}

impl FocusRequest {
    /// Whether a view is waiting to be published.
    pub(crate) fn is_pending(&self) -> bool {
        !matches!(self, FocusRequest::None)
    }

    /// Asks for the current view again. A waiting target stays, it is published
    /// anyway.
    pub(crate) fn republish(&mut self) {
        if let FocusRequest::None = self {
            *self = FocusRequest::Republish;
        }
    }

    /// The view to publish for a widget showing `current` at `view_size` pixels.
    /// Computed anew on every call: the canvas publishes again with each event until
    /// the first view is applied, and all of these messages have to agree.
    pub(crate) fn focal_point(&self, current: FocalPoint, view_size: Size) -> FocalPoint {
        match self {
            FocusRequest::Target(target) => resolve_target(target, current, view_size),
            FocusRequest::None | FocusRequest::Republish => current,
        }
    }
}

/// Where a focus on `target` ends, for a widget showing `current` at `view_size`
/// pixels.
///
/// An area is fitted per axis: each axis gives the level at which the area's extent
/// plus its pixel padding is `fill` of the widget's width or height, and the smaller
/// level is binding, so an elongated area fits as well. The centre is taken in
/// mercator space, where it is the visual centre of the area on the map. An area
/// built from nothing has no position, the view stays where it is then.
pub(crate) fn resolve_target(
    target: &FocusTarget,
    current: FocalPoint,
    view_size: Size,
) -> FocalPoint {
    let (bounds, fill) = match target {
        FocusTarget::Position(position) => {
            return FocalPoint {
                position: *position,
                ..current
            };
        }
        FocusTarget::Area { bounds, fill } => (bounds, *fill as f64),
    };

    let top_left =
        LatitudeLongitude::new(bounds.min_max_lat.1, bounds.min_max_long.0).get_tile_coordinates(0);
    let bottom_right =
        LatitudeLongitude::new(bounds.min_max_lat.0, bounds.min_max_long.1).get_tile_coordinates(0);
    let position = TileCoordinates {
        x: (top_left.x + bottom_right.x) * 0.5,
        y: (top_left.y + bottom_right.y) * 0.5,
        zoom: 0,
    }
    .into();

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
    use crate::gui_system::map_widget::focus_animation::pixel_offset;

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

    /// An area around Trier, `half_lat` and `half_long` degrees to each side.
    fn area(half_lat: f64, half_long: f64, fill: f32) -> FocusTarget {
        FocusTarget::Area {
            bounds: MercatorRectangle {
                min_max_lat: (49.75 - half_lat, 49.75 + half_lat),
                min_max_long: (6.63 - half_long, 6.63 + half_long),
                pixel_padding: 0.0,
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
        let (dx, dy) = pixel_offset(
            &at_view,
            &corner(bounds.min_max_lat.0, bounds.min_max_long.1),
        );
        (dx.abs(), dy.abs())
    }

    #[test]
    fn wide_area_fits_the_width() {
        let target = area(0.01, 1.0, 0.8);
        let end = resolve_target(&target, trier(), VIEW);
        let (width, height) = extent_in_pixels(&target, &end);
        assert!(
            (width - VIEW.width as f64 * 0.8).abs() < 0.5,
            "width {width}"
        );
        assert!(height < VIEW.height as f64 * 0.8);
    }

    #[test]
    fn tall_area_fits_the_height() {
        let target = area(1.0, 0.01, 0.8);
        let end = resolve_target(&target, trier(), VIEW);
        let (width, height) = extent_in_pixels(&target, &end);
        assert!(
            (height - VIEW.height as f64 * 0.8).abs() < 0.5,
            "height {height}"
        );
        assert!(width < VIEW.width as f64 * 0.8);
        // The centre is the mercator centre, which lies north of the mean latitude.
        assert!(end.position.latitude > 49.75);
    }

    #[test]
    fn circle_fills_the_shorter_side() {
        // Mercator is conformal, so the box of a small circle is square on screen, and
        // the height is the binding side of VIEW.
        let target = FocusTarget::around(LatitudeLongitude::new(49.75, 6.63), 200.0);
        let end = resolve_target(&target, trier(), VIEW);
        let (width, height) = extent_in_pixels(&target, &end);
        let expected = VIEW.height as f64 * MapWidgetSystem::DEFAULT_FOCUS_FILL as f64;
        assert!((height - expected).abs() < 0.5, "height {height}");
        assert!((width - height).abs() < 0.5, "width {width}");
        assert!((end.position.latitude - 49.75).abs() < 1e-6);
        assert!((end.position.longitude - 6.63).abs() < 1e-9);
    }

    #[test]
    fn tiny_area() {
        let point = resolve_target(&area(0.0, 0.0, 0.8), trier(), VIEW);
        assert_eq!(point.continuous_zoom_level, MAXIMUM_AREA_ZOOM_LEVEL);
    }
}
