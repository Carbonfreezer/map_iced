//! This module covers the outside visible coordinate systems,
//! which ius currently the latitude longitude representation.


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
}

