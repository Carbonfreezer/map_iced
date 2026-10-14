//! This module contains the latitude longitude positional representation that is visible to
//! the outside in this crate.

use crate::gui_system::internal_math::{TileCoordinates, get_scaling_factor};
use std::f64::consts::PI;

/// The boundary latitude we do not overshoot. Needed
/// because of distortion artifacts in the mercator projection.
pub(crate) const BOUNDARY_LATITUDE: f64 = 85.05112878;

/// The absolute minimum and maximum representation of the longitude.
pub(crate) const BOUNDARY_LONGITUDE: f64 = 180.0;

/// The radius of the sphere Web Mercator is defined on, in metres.
const EARTH_RADIUS: f64 = 6_378_137.0;

/// The circumference of the earth in meters for mercator calculations..
pub(crate) const EARTH_CIRCUMFERENCE: f64 = 2.0 * PI * EARTH_RADIUS;

/// The latitude longitude pair. Both are given in degrees.
/// (latitude: -BOUNDARY_LATITUDE .. BOUNDARY_LATITUDE, longitude: -180 .. 180)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatitudeLongitude {
    /// Latitude in degrees
    pub latitude: f64,
    /// Longitude in degrees
    pub longitude: f64,
}

impl LatitudeLongitude {
    /// Constructs the object and makes sure, that both coordinates are in the valid range
    /// (latitude: -BOUNDARY_LATITUDE .. BOUNDARY_LATITUDE, longitude: -180 .. 180)
    pub const fn new(latitude: f64, longitude: f64) -> Self {
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
    /// The ring is open: it has `num_points` entries, the closing edge from the last
    /// point back to the first is implied. Fewer than three points make no ring.
    ///
    /// The points run counter-clockwise with longitude as x and latitude as y, north,
    /// west, south, east, the usual orientation of an outer ring (GeoJSON, `geo`). In
    /// screen coordinates, where y points down, the same ring has a positive area.
    ///
    /// Every point is the destination of walking `radius_meter` along a great circle in its
    /// bearing, see [`Self::destinations`].
    pub(crate) fn create_circle_around(
        &self,
        radius_meter: f64,
        num_points: usize,
    ) -> Vec<LatitudeLongitude> {
        assert!(num_points >= 3, "Needed at least 3 points for a circle.");
        // A growing bearing turns clockwise, north to east. It is negated so that the
        // ring runs counter-clockwise, north to west.
        self.destinations(
            radius_meter,
            (0..num_points).map(|i| -(i as f64) / num_points as f64 * 2.0 * PI),
        )
        .collect()
    }

    /// Builds the position points of a circle sector with this position as its apex.
    /// `heading_degree` is the compass bearing the sector points to, clockwise from
    /// north, and `opening_degree` the angle of the fan, centred on the heading.
    ///
    /// The ring is open and runs counter-clockwise like [`Self::create_circle_around`]:
    /// the apex first, then `num_arc_points` points on the arc from the clockwise edge
    /// of the fan to the counter-clockwise one. Every arc point is `radius_meter` away
    /// from the apex along a great circle.
    pub(crate) fn create_sector_around(
        &self,
        radius_meter: f64,
        heading_degree: f64,
        opening_degree: f64,
        num_arc_points: usize,
    ) -> Vec<LatitudeLongitude> {
        assert!(num_arc_points >= 2, "Needed at least 2 points for an arc.");
        let first = (heading_degree + opening_degree * 0.5).to_radians();
        let step = opening_degree.to_radians() / (num_arc_points - 1) as f64;
        std::iter::once(*self)
            .chain(self.destinations(
                radius_meter,
                (0..num_arc_points).map(|i| first - i as f64 * step),
            ))
            .collect()
    }

    /// The points reached by walking `radius_meter` from here along a great circle in
    /// each of the `bearings`, given in radians clockwise from north. That stays correct
    /// near the poles, where a squeezed circle breaks down.
    ///
    /// The map does not wrap around at the date line, so a point beyond it is clamped
    /// to the map edge by [`Self::new`], the same as it gets clipped there on screen.
    fn destinations(
        &self,
        radius_meter: f64,
        bearings: impl Iterator<Item = f64>,
    ) -> impl Iterator<Item = LatitudeLongitude> {
        let angular_distance = radius_meter / EARTH_RADIUS;
        let (sin_distance, cos_distance) = angular_distance.sin_cos();
        let (sin_latitude, cos_latitude) = self.latitude.to_radians().sin_cos();
        // On the unit sphere, with δ = angular_distance, θ = bearing (0 north, π/2 east)
        // and the centre at latitude φ and longitude λ, every point is
        //
        //     q = R_z(λ) · R_y(φ) · (cos δ, sin δ · sin θ, sin δ · cos θ)ᵀ
        //
        // The vector on the right is the circle around (0°, 0°), where east is +y and
        // north is +z. R_y(φ) tilts it north to latitude φ:
        //
        //            ⎡ cos φ   0   −sin φ ⎤
        //   R_y(φ) = ⎢   0     1     0    ⎥
        //            ⎣ sin φ   0    cos φ ⎦
        //
        // R_z(λ) turns about the earth's axis, so it only adds λ to the longitude.
        // Multiplied out, before R_z:
        //
        //   q_x = cos φ · cos δ − sin φ · sin δ · cos θ
        //   q_y = sin δ · sin θ
        //   q_z = sin φ · cos δ + cos φ · sin δ · cos θ
        //
        // The latitude is asin(q_z), the longitude offset atan2(q_y, q_x). Both atan2
        // arguments below are scaled by cos φ, which reuses sin(lat) = q_z:
        // cos δ − sin φ · q_z = cos φ · q_x. atan2 ignores a common positive factor,
        // and cos φ > 0 because the latitude stays within BOUNDARY_LATITUDE.
        let longitude = self.longitude;
        bearings.map(move |bearing| {
            let lat =
                (sin_latitude * cos_distance + cos_latitude * sin_distance * bearing.cos()).asin();
            let long_diff = (bearing.sin() * sin_distance * cos_latitude)
                .atan2(cos_distance - sin_latitude * lat.sin());
            LatitudeLongitude::new(lat.to_degrees(), longitude + long_diff.to_degrees())
        })
    }

    /// Uses the harvesine formula to get the distance to another point in meters.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let first_point = LatitudeLongitude::new(49.7554, 6.6436);
    /// let second_point = LatitudeLongitude::new( 49.7540, 6.6100);
    /// let dist = first_point.get_distance_to(second_point);
    /// ```
    pub fn get_distance_to(&self, other_point: Self) -> f64 {
        let (lat_a, lat_b) = (
            self.latitude.to_radians(),
            other_point.latitude.to_radians(),
        );
        let d_lat = lat_b - lat_a;
        let d_long = (other_point.longitude - self.longitude).to_radians();
        let h =
            (d_lat * 0.5).sin().powi(2) + lat_a.cos() * lat_b.cos() * (d_long * 0.5).sin().powi(2);
        // Rounding can push h just above 1 for near-antipodal points, where asin gives NaN.
        2.0 * EARTH_RADIUS * h.sqrt().min(1.0).asin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, proptest};

    proptest! {
        #[test]
        fn circle_points_keep_their_distance(latitude in -80.0f64..80.0, longitude in -177.0f64..177.0, radius in 0.0 .. 50_000.0) {
            // Keeps the ring off the map edges, where it gets clamped: 50 km is at most
            // ~0.45 degrees of latitude, and ~2.6 degrees of longitude at 80 degrees.
            let centre = LatitudeLongitude::new(latitude, longitude);
            let ring = centre.create_circle_around(radius, 32);
            for point in &ring {
                prop_assert!(point.latitude.is_finite() && point.longitude.is_finite());
                let distance = centre.get_distance_to(*point);
                // The absolute micrometre covers rounding noise for a zero radius.
                prop_assert!((distance - radius).abs() < radius * 1e-6 + 1e-6, "{latitude}: {distance} vs {radius}");
            }
        }

        #[test]
        fn sector_is_apex_plus_arc_counter_clockwise(latitude in -80.0f64..80.0, longitude in -177.0f64..177.0, radius in 100.0 .. 50_000.0, heading in -360.0f64..720.0, opening in 1.0f64..=180.0) {
            let apex = LatitudeLongitude::new(latitude, longitude);
            let ring = apex.create_sector_around(radius, heading, opening, 16);
            prop_assert!(ring.len() == 17);
            prop_assert!(ring[0].latitude == apex.latitude && ring[0].longitude == apex.longitude);
            for point in &ring[1..] {
                let distance = apex.get_distance_to(*point);
                prop_assert!((distance - radius).abs() < radius * 1e-6, "{distance} vs {radius}");
            }
            let doubled_area: f64 = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .map(|(a, b)| a.longitude * b.latitude - b.longitude * a.latitude)
                .sum();
            prop_assert!(doubled_area > 0.0, "{latitude}, {longitude}, {heading}, {opening}: {doubled_area}");
        }

        #[test]
        fn circle_runs_counter_clockwise(latitude in -80.0f64..80.0, longitude in -177.0f64..177.0, radius in 100.0 .. 50_000.0) {
            let ring = LatitudeLongitude::new(latitude, longitude).create_circle_around(radius, 32);
            // Twice the signed area by the shoelace formula, longitude as x, latitude as y.
            let doubled_area: f64 = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .map(|(a, b)| a.longitude * b.latitude - b.longitude * a.latitude)
                .sum();
            prop_assert!(doubled_area > 0.0, "{latitude}, {longitude}: {doubled_area}");
            // The second point lies west of the first, which is due north.
            prop_assert!(ring[1].longitude < ring[0].longitude);
        }

        #[test]
        fn antipodal_distance_is_half_the_circumference(latitude in -BOUNDARY_LATITUDE..BOUNDARY_LATITUDE, longitude in -BOUNDARY_LONGITUDE..0.0) {
            // The longitude range keeps the antipode inside -180 .. 180, where new would clamp it.
            // Near h = 1 the haversine is ill-conditioned: asin(1 - e) is about pi/2 - sqrt(2e),
            // so rounding noise in h costs tenths of a metre here.
            let point = LatitudeLongitude::new(latitude, longitude);
            let antipode = LatitudeLongitude::new(-latitude, longitude + 180.0);
            let distance = point.get_distance_to(antipode);
            prop_assert!((distance - PI * EARTH_RADIUS).abs() < 1.0, "{latitude}, {longitude}: {distance}");
        }
    }

    #[test]
    fn east_west_distance_shrinks_with_latitude() {
        // One degree along the 60th parallel is R * cos(60°) * 1° long; the great circle
        // between its ends is shorter only by about 2e-5 of that. Squaring the cosine
        // factor in the haversine would halve the result here, while the equator hides it.
        let distance =
            LatitudeLongitude::new(60.0, 0.0).get_distance_to(LatitudeLongitude::new(60.0, 1.0));
        let parallel_arc = EARTH_RADIUS * 60f64.to_radians().cos() * 1f64.to_radians();
        assert!(distance <= parallel_arc, "{distance} vs {parallel_arc}");
        assert!(
            distance > parallel_arc * (1.0 - 1e-4),
            "{distance} vs {parallel_arc}"
        );
    }

    #[test]
    fn near_antipodal_rounding_does_not_give_nan() {
        // Found by random search, about one pair in 15 million: h rounds two ULP above 1,
        // so its square root lands above 1 as well and asin would give NaN without the clamp.
        let point = LatitudeLongitude::new(57.36396000882365, -64.65731757479259);
        let antipode = LatitudeLongitude::new(-57.36396001600441, 115.34268263197147);
        let distance = point.get_distance_to(antipode);
        assert!((distance - PI * EARTH_RADIUS).abs() < 1.0, "{distance}");
    }
}
