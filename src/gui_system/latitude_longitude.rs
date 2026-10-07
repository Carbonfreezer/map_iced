//! This module contains the latitude longitude positional representation that is visible to
//! the outside in this crate.

use crate::gui_system::internal_math::{TileCoordinates, get_scaling_factor};
use std::f64::consts::PI;

/// The boundary latitude we do not overshoot. Needed
/// because of distortion artifacts in the mercator projection.
pub const BOUNDARY_LATITUDE: f64 = 85.05112878;

/// The boundary longitude we have.
pub const BOUNDARY_LONGITUDE: f64 = 180.0;

/// The radius of the sphere Web Mercator is defined on, in metres.
const EARTH_RADIUS: f64 = 6_378_137.0;


/// The circumference of the earth we have.
pub const EARTH_CIRCUMFERENCE: f64 = 2.0 * PI * EARTH_RADIUS;


/// The latitude longitude pair. Both are given in degrees.
/// (latitude: -BOUNDARY_LATITUDE .. BOUNDARY_LATITUDE, longitude: -180 .. 180)
#[derive(Debug, Clone, Copy)]
pub struct LatitudeLongitude {
    /// Latitude in degrees
    pub latitude: f64,
    /// Longitude in degrees
    pub longitude: f64,
}

impl LatitudeLongitude {
    /// Constructs the object and makes sure, that both coordinates are in the valid range
    /// (latitude: -BOUNDARY_LATITUDE .. BOUNDARY_LATITUDE, longitude: -180 .. 180)
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude: latitude.clamp(-BOUNDARY_LATITUDE, BOUNDARY_LATITUDE),
            longitude: longitude.clamp(-BOUNDARY_LONGITUDE, BOUNDARY_LONGITUDE),
        }
    }

    /// Gets the tile coordinates in the indicated zoom level
    pub(crate) fn get_tile_coordinates(&self, zoom: u8) -> TileCoordinates {
        let scaling = get_scaling_factor(zoom);

        let x = (self.longitude + 180.0) / 360.0 * scaling;
        let angle = self.latitude * PI / 180.0;
        let y = (1.0 - f64::ln(f64::tan(angle) + 1.0 / f64::cos(angle)) / PI) * scaling * 0.5;

        TileCoordinates { x, y, zoom }
    }
    
    /// Builds the position points of a circle around the position with the radius indicated in meters.
    /// The ring is closed, the last point repeats the first, so it has `num_points + 1` entries.
    /// Fewer than three points make no ring, the result is empty then.
    ///
    /// Every point is the destination of walking `radius_meter` along a great circle in its
    /// bearing, which stays correct near the poles where a squeezed circle breaks down.
    /// The map does not wrap around at the date line, so a circle crossing it is clamped
    /// to the map edge by [`Self::new`], the same as it gets clipped there on screen.
    pub(crate) fn create_circle_around(&self, radius_meter : f64, num_points: usize) -> Vec<LatitudeLongitude> {
        if num_points < 3 {
            return Vec::new();
        }
        let angular_distance = radius_meter / EARTH_RADIUS;
        let (sin_distance, cos_distance) = angular_distance.sin_cos();
        let (sin_latitude, cos_latitude) = self.latitude.to_radians().sin_cos();
        (0..=num_points).map(|i| {
            let bearing = i as f64 / num_points as f64 * 2.0 * PI;
            let lat = (sin_latitude * cos_distance + cos_latitude * sin_distance * bearing.cos()).asin();
            let long_diff = (bearing.sin() * sin_distance * cos_latitude)
                .atan2(cos_distance - sin_latitude * lat.sin());
            LatitudeLongitude::new(lat.to_degrees(), self.longitude + long_diff.to_degrees())
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use proptest::{prop_assert, proptest};
    use super::*;

    /// Great circle distance in metres, as an independent check.
    fn haversine(a: LatitudeLongitude, b: LatitudeLongitude) -> f64 {
        let (lat_a, lat_b) = (a.latitude.to_radians(), b.latitude.to_radians());
        let d_lat = lat_b - lat_a;
        let d_long = (b.longitude - a.longitude).to_radians();
        let h = (d_lat * 0.5).sin().powi(2) + lat_a.cos() * lat_b.cos() * (d_long * 0.5).sin().powi(2);
        2.0 * EARTH_RADIUS * h.sqrt().asin()
    }

    #[test]
    fn circle_needs_three_points() {
        let centre = LatitudeLongitude::new(50.0, 8.0);
        assert!(centre.create_circle_around(100.0, 0).is_empty());
        assert!(centre.create_circle_around(100.0, 2).is_empty());
        assert_eq!(centre.create_circle_around(100.0, 3).len(), 4);
    }

    proptest! {
        #[test]
        fn circle_points_keep_their_distance(latitude in -80.0f64..80.0, longitude in -177.0f64..177.0, radius in 0.0 .. 50_000.0) {
            // Keeps the ring off the map edges, where it gets clamped: 50 km is at most
            // ~0.45 degrees of latitude, and ~2.6 degrees of longitude at 80 degrees.
            let centre = LatitudeLongitude::new(latitude, longitude);
            let ring = centre.create_circle_around(radius, 32);
            for point in &ring {
                prop_assert!(point.latitude.is_finite() && point.longitude.is_finite());
                let distance = haversine(centre, *point);
                // The absolute micrometre covers rounding noise for a zero radius.
                prop_assert!((distance - radius).abs() < radius * 1e-6 + 1e-6, "{latitude}: {distance} vs {radius}");
            }
            let (first, last) = (ring[0], ring[ring.len() - 1]);
            prop_assert!(haversine(first, last) < 1e-6, "ring not closed");
        }
    }
}
