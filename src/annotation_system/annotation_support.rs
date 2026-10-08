//! Contains supporting functionality for annotation.

use crate::annotation_system::waypoint_system::WaypointKey;
use crate::gui_system::latitude_longitude::{
    BOUNDARY_LATITUDE, BOUNDARY_LONGITUDE, LatitudeLongitude,
};

/// Indicates where a  label should be positioned.
pub enum LabelPosition {
    /// No label at all.
    NoLabel,
    /// Show it at mouse position.
    AtMousePosition,
    /// Show it at a fix geolocation.
    FixGeoLocation(LatitudeLongitude),
}

/// A trait that gets implemented by all annotation features.
pub(crate) trait Annotation {
    /// Asks for the annotation description, can process the cursor being handed over in latitude longitude
    /// coordinates, for instance if the annotation should change on cursor position (for instance on a track)
    fn description(&self, cursor: LatitudeLongitude) -> &str;
    /// Gets the colling boundaries of the system.
    fn cull_bounds(&self) -> MercatorRectangle;
    /// Does a hit test if a position handed over. This has be to combined with a test against the cull bounds
    /// upfront. For a waypoint this is always true. For a region or path that can be used as a filter to
    /// only hit test against specific regions.
    fn hit_test_specific(&self, position: LatitudeLongitude) -> bool;
    /// Gets the anchor position of the label.
    fn label_anchor(&self, cursor: LatitudeLongitude) -> LabelPosition;
}

/// The generalized form of annotation keys. Can be used to query all annotations.
pub enum AnnotationKey {
    /// The key for the way points.
    Waypoint(WaypointKey),
}

/// The mercator rectangle an annotation feature covers on the map
/// with extra pixel padding.
#[derive(Debug, Clone)]
pub struct MercatorRectangle {
    /// The minimum and maximum latitude we cover.
    pub min_max_lat: (f64, f64),
    /// The minimum and maximum longitude we cover.
    pub min_max_long: (f64, f64),
    /// The extra pixel padding in every dimension.
    pub pixel_padding: f64,
    /// Indicates if the rectangle is valid, may become invalid if constructed from an empty set.
    pub is_valid: bool,
}

/// The top left bottom right position to be able to check with the Rectangle of the viewport later on.
#[derive(Debug, Clone)]
pub struct TopLeftBottomRight {
    pub top_left: LatitudeLongitude,
    pub bottom_right: LatitudeLongitude,
}

/// Converts the extensions of the meractor rectangle
impl From<&MercatorRectangle> for TopLeftBottomRight {
    fn from(rect: &MercatorRectangle) -> Self {
        Self {
            top_left: LatitudeLongitude::new(rect.min_max_lat.1, rect.min_max_long.0),
            bottom_right: LatitudeLongitude::new(rect.min_max_lat.0, rect.min_max_long.1),
        }
    }
}

impl MercatorRectangle {
    /// Crates a mercator rectangle from a position like used for the way points.
    pub fn create_from_position(
        position: LatitudeLongitude,
        pixel_padding: f64,
    ) -> MercatorRectangle {
        Self {
            min_max_lat: (position.latitude, position.latitude),
            min_max_long: (position.longitude, position.longitude),
            pixel_padding,
            is_valid: true,
        }
    }

    /// Crates the mercator rectangle from a position array as it may be used by a way path or region marked by gps points.
    /// Returns None for an empty array, there is nothing to cover then.
    pub fn create_from_position_array(
        position_array: impl Iterator<Item = LatitudeLongitude>,
        pixel_padding: f64,
    ) -> MercatorRectangle {
        let (min_lat, max_lat, min_long, max_long) = position_array.fold(
            (
                BOUNDARY_LATITUDE,
                -BOUNDARY_LATITUDE,
                BOUNDARY_LONGITUDE,
                -BOUNDARY_LONGITUDE,
            ),
            |(min_lat, max_lat, min_long, max_long), position| {
                (
                    min_lat.min(position.latitude),
                    max_lat.max(position.latitude),
                    min_long.min(position.longitude),
                    max_long.max(position.longitude),
                )
            },
        );
        Self {
            min_max_lat: (min_lat, max_lat),
            min_max_long: (min_long, max_long),
            pixel_padding,
            is_valid: max_lat >= min_lat && max_long >= min_long,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_position_array_has_no_rectangle() {
        assert!(!MercatorRectangle::create_from_position_array(std::iter::empty(), 0.0).is_valid);
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
        );
        assert_eq!(rect.min_max_lat, (48.0, 52.0));
        assert_eq!(rect.min_max_long, (8.0, 11.0));
    }
}
