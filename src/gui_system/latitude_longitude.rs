//! This module contains the latitude longitude positional representation that is visible to
//! the outside in this crate.

use crate::gui_system::internal_math::{TileCoordinates, get_scaling_factor};
use std::f64::consts::PI;

/// The boundary latitude we do not overshoot. Needed
/// because of distortion artifacts in the mercator projection.
pub const BOUNDARY_LATITUDE: f64 = 85.05112878;

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
            longitude: longitude.clamp(-180.0, 180.0),
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
}
