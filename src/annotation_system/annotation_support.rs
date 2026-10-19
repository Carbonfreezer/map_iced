//! Contains supporting functionality for annotation.

use crate::annotation_system::path_system::PathKey;
use crate::annotation_system::region_system::RegionKey;
use crate::annotation_system::waypoint_system::WaypointKey;
use crate::gui_system::internal_math::MercatorPoint;
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::focus::FocusTarget;
use iced::Color;
use iced::advanced::image::Handle;
use std::fmt::Debug;

/// Indicates where a  label should be positioned.
pub(crate) enum LabelPosition {
    /// Show it at mouse position.
    AtMousePosition,
    /// Show it at a fix geolocation.
    FixGeoLocation(LatitudeLongitude),
}

/// A trait that gets implemented by all annotation features.
pub(crate) trait Annotation: Debug {
    /// Asks for the annotation description, can process the cursor being handed over in latitude longitude
    /// coordinates, for instance if the annotation should change on cursor position (for instance on a track)
    fn description(&self, cursor: LatitudeLongitude) -> Option<String>;
    /// Gets the culling boundaries of the system, should be precomputed.
    fn cull_bounds(&self) -> &MercatorRectangle;
    /// The point the annotation stands for: direction arrows point at it and
    /// [`MapWidgetSystem::center`] reports it. By default the centre of the cull
    /// bounds, which for a way point is its position.
    ///
    /// [`MapWidgetSystem::center`]: crate::gui_system::map_widget_system::MapWidgetSystem::center
    fn get_center(&self) -> LatitudeLongitude {
        self.cull_bounds().get_center()
    }
    /// What a focus on the annotation aims at. By default its cull bounds, covering
    /// the share `fill` of the widget; an annotation without extent overrides this
    /// with its position.
    fn focus_target(&self, fill: f32) -> FocusTarget {
        FocusTarget::Area {
            bounds: self.cull_bounds().clone(),
            fill,
        }
    }
    /// The point of the annotation closest to `position`, measured Euclidean in
    /// Mercator space, or [`ClosestPoint::Inside`] if `position` lies on or inside it.
    /// The widget turns the distance to it into pixels and compares that against the
    /// pixel padding of the [`Self::cull_bounds`], the fine test after the coarse one.
    ///
    /// Only called after `position` passed the test against [`Self::cull_bounds`],
    /// so an annotation without extent (a way point) or without usable geometry can
    /// answer `Inside` and the padded bounds become its hit area.
    fn closest_point(&self, position: MercatorPoint) -> ClosestPoint;
    /// Gets the anchor position of the label.
    fn label_anchor(&self, cursor: LatitudeLongitude) -> LabelPosition;
    /// The direction arrow, if the annotation is flagged.
    fn get_flag(&self) -> Option<DirectionFlag>;
    /// This method returns all information to render the specific object.
    fn get_render_information(&self) -> RenderingInformation<'_>;
    /// Clones the annotation into a box, so the widgets can take a snapshot of it.
    fn clone_box(&self) -> Box<dyn Annotation>;
}

/// The answer of [`Annotation::closest_point`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ClosestPoint {
    /// The position lies on or inside the annotation, it is hit whatever the padding.
    Inside,
    /// The point of the annotation nearest to the position, in Mercator space.
    Nearest(MercatorPoint),
}

/// The generalized form of annotation keys. Can be used to query all annotations.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum AnnotationKey {
    /// The key for the way points.
    Waypoint(WaypointKey),
    /// The key for regions.
    Region(RegionKey),
    /// The key for paths.
    Path(PathKey),
}

/// The errors of the methods that take an [`AnnotationKey`] of any kind. A method
/// that fails changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationError {
    /// There is no annotation with this key, or not any more.
    KeyNotContained,
}

impl From<WaypointKey> for AnnotationKey {
    fn from(key: WaypointKey) -> Self {
        AnnotationKey::Waypoint(key)
    }
}

impl From<RegionKey> for AnnotationKey {
    fn from(key: RegionKey) -> Self {
        AnnotationKey::Region(key)
    }
}

impl From<PathKey> for AnnotationKey {
    fn from(key: PathKey) -> Self {
        AnnotationKey::Path(key)
    }
}

/// Contains the information for rendering a closed shape, with positions handed over in CCW.
#[derive(Debug, Clone)]
pub(crate) struct RenderingClosedShape {
    /// The positions in latitude longitude, counter-clockwise with longitude as x and
    /// latitude as y. The ring is open, the last point gets connected to the first one.
    pub(crate) positions_ccw: Vec<LatitudeLongitude>,
    /// The color we use to fill the core.
    pub(crate) core_color: Color,
    /// The color we use for the border Color.
    pub(crate) border_color: Color,
}

/// The information needed to render an annotation element.
pub(crate) enum RenderingInformation<'a> {
    /// Image with raw image data, from an image file. (used in waypoint)
    Image(Handle),
    /// A cross we want to draw with an indicated color. (used in waypoint)
    Cross(Color),
    /// This is a closed shape as a polygon.
    Polygon(&'a RenderingClosedShape),
    /// An open line through the positions, in one colour. (used in path)
    Line {
        positions: &'a [LatitudeLongitude],
        color: Color,
    },
}

/// The mercator rectangle an annotation feature covers on the map
/// with extra pixel padding. It always covers at least one position, the
/// constructors keep the minimum below or at the maximum.
#[derive(Debug, Clone)]
pub struct MercatorRectangle {
    /// The minimum and maximum latitude we cover.
    pub(crate) min_max_lat: (f64, f64),
    /// The minimum and maximum longitude we cover.
    pub(crate) min_max_long: (f64, f64),
    /// The extra pixel padding in every dimension.
    pub(crate) pixel_padding: f64,
}

/// The corners of a [`MercatorRectangle`] in latitude longitude.
#[derive(Debug, Clone)]
pub(crate) struct TopLeftBottomRight {
    pub(crate) top_left: LatitudeLongitude,
    pub(crate) bottom_right: LatitudeLongitude,
}

/// Converts the extensions of the mercator rectangle.
impl From<&MercatorRectangle> for TopLeftBottomRight {
    fn from(rect: &MercatorRectangle) -> Self {
        Self {
            top_left: LatitudeLongitude::new(rect.min_max_lat.1, rect.min_max_long.0),
            bottom_right: LatitudeLongitude::new(rect.min_max_lat.0, rect.min_max_long.1),
        }
    }
}

impl MercatorRectangle {
    /// Creates a mercator rectangle from a single position, as used for the way points.
    ///
    /// # Example
    /// ```
    /// use map_iced::annotation_system::annotation_support::MercatorRectangle;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let bounds = MercatorRectangle::create_from_position(LatitudeLongitude::new(49.75, 6.63), 10.0);
    /// assert_eq!(bounds.get_center(), LatitudeLongitude::new(49.75, 6.63));
    /// ```
    pub fn create_from_position(
        position: LatitudeLongitude,
        pixel_padding: f64,
    ) -> MercatorRectangle {
        Self {
            min_max_lat: (position.latitude, position.latitude),
            min_max_long: (position.longitude, position.longitude),
            pixel_padding,
        }
    }

    /// Creates the mercator rectangle from a position array, as used by paths and regions.
    /// An empty array gives `None`, there is nothing to cover then.
    ///
    /// # Example
    /// ```
    /// use map_iced::annotation_system::annotation_support::MercatorRectangle;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::{FocusTarget, MapWidgetSystem};
    /// // The area of an operation, to focus a widget on it.
    /// let positions = [LatitudeLongitude::new(49.70, 6.60), LatitudeLongitude::new(49.80, 6.70)];
    /// let bounds = MercatorRectangle::create_from_position_array(positions.into_iter(), 0.0).expect("not empty");
    /// let target = FocusTarget::Area { bounds, fill: MapWidgetSystem::DEFAULT_FOCUS_FILL };
    ///
    /// assert!(MercatorRectangle::create_from_position_array(std::iter::empty(), 0.0).is_none());
    /// ```
    pub fn create_from_position_array(
        mut position_array: impl Iterator<Item = LatitudeLongitude>,
        pixel_padding: f64,
    ) -> Option<MercatorRectangle> {
        let first = position_array.next()?;
        Some(position_array.fold(
            Self::create_from_position(first, pixel_padding),
            |rect, position| Self {
                min_max_lat: (
                    rect.min_max_lat.0.min(position.latitude),
                    rect.min_max_lat.1.max(position.latitude),
                ),
                min_max_long: (
                    rect.min_max_long.0.min(position.longitude),
                    rect.min_max_long.1.max(position.longitude),
                ),
                ..rect
            },
        ))
    }

    /// Creates the mercator rectangle that encloses the circle with `radius_meter`
    /// around `center`, e.g. the surroundings of a vehicle to keep in view. The circle
    /// is measured on the sphere, so the rectangle stays tight at high latitudes, where
    /// it spans more degrees of longitude. A circle over a pole spans all longitudes.
    ///
    /// # Example
    /// ```
    /// use map_iced::annotation_system::annotation_support::MercatorRectangle;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let trier = LatitudeLongitude::new(49.75, 6.63);
    /// let bounds = MercatorRectangle::create_around(trier, 200.0, 0.0);
    /// // The centre stays where it is; 200 m are about 0.0018 degrees of latitude.
    /// assert!((bounds.get_center().longitude - 6.63).abs() < 1e-9);
    /// assert!((bounds.get_center().latitude - 49.75).abs() < 1e-6);
    /// ```
    pub fn create_around(
        center: LatitudeLongitude,
        radius_meter: f64,
        pixel_padding: f64,
    ) -> MercatorRectangle {
        Self::create_from_position_array(
            center.circle_extremes(radius_meter).into_iter(),
            pixel_padding,
        )
        .expect("Four extreme points")
    }

    /// The centre of the rectangle, by default the centre of an annotation.
    ///
    /// # Example
    /// ```
    /// use map_iced::annotation_system::annotation_support::MercatorRectangle;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let positions = [LatitudeLongitude::new(49.70, 6.60), LatitudeLongitude::new(49.80, 6.70)];
    /// let bounds = MercatorRectangle::create_from_position_array(positions.into_iter(), 0.0).expect("not empty");
    /// let center = bounds.get_center();
    /// assert!((center.latitude - 49.75).abs() < 1e-9);
    /// assert!((center.longitude - 6.65).abs() < 1e-9);
    /// ```
    pub fn get_center(&self) -> LatitudeLongitude {
        LatitudeLongitude::new(
            (self.min_max_lat.0 + self.min_max_lat.1) * 0.5,
            (self.min_max_long.0 + self.min_max_long.1) * 0.5,
        )
    }

    /// Asks specifically for the pixel padding.
    ///
    /// # Example
    /// ```
    /// use map_iced::annotation_system::annotation_support::MercatorRectangle;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let bounds = MercatorRectangle::create_from_position(LatitudeLongitude::new(49.75, 6.63), 10.0);
    /// assert_eq!(bounds.pixel_padding(), 10.0);
    /// ```
    pub fn pixel_padding(&self) -> f32 {
        self.pixel_padding as f32
    }
}

/// How the direction arrow of a flagged annotation component looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionFlag {
    /// The fill colour of the arrow.
    pub color: Color,
    /// Where several arrows overlap, the higher priority is drawn on top and wins
    /// the click. They are not spread apart, that would bend their direction.
    pub priority: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_position_array_has_no_rectangle() {
        assert!(MercatorRectangle::create_from_position_array(std::iter::empty(), 0.0).is_none());
    }

    #[test]
    fn position_array_rectangle_spans_all_points() {
        let rect = MercatorRectangle::create_from_position_array(
            [
                LatitudeLongitude::new(50.0, 8.0),
                LatitudeLongitude::new(48.0, 11.0),
                LatitudeLongitude::new(52.0, 9.0),
            ]
            .into_iter(),
            3.0,
        )
        .unwrap();
        assert_eq!(rect.min_max_lat, (48.0, 52.0));
        assert_eq!(rect.min_max_long, (8.0, 11.0));
    }
}
