//! This module contains all related to math and coordinates.

use iced::{Rectangle, Vector};
use itertools::iproduct;
use std::f64::consts::PI;

/// The maximum zoom level we allow.
pub const MAXIMUM_ZOOM_LEVEL: u8 = 19;

/// the size of a tile in pixel coordinates.
pub const TILE_SIZE_PIXEL: u32 = 256;

/// The tile coordinates in float space,
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TileCoordinates {
    pub x: f64,
    pub y: f64,
    pub zoom: u8,
}

/// A position of the tile in rounded coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TilePosition {
    pub x: u32,
    pub y: u32,
    pub zoom: u8,
}

impl TilePosition {
    fn check_sanity(&self) {
        debug_assert!(
            (0..=MAXIMUM_ZOOM_LEVEL).contains(&self.zoom),
            "zoom out of range"
        );
        let max_value = (1u32 << self.zoom) - 1;
        debug_assert!((0..=max_value).contains(&self.x));
        debug_assert!((0..=max_value).contains(&self.y));
    }
}

impl From<TileCoordinates> for TilePosition {
    /// Along the way we clamp to the legal range.
    fn from(value: TileCoordinates) -> Self {
        debug_assert!(
            (0..=MAXIMUM_ZOOM_LEVEL).contains(&value.zoom),
            "zoom out of range"
        );
        let max_value = ((1u32 << value.zoom) - 1) as f64;

        Self {
            x: value.x.floor().clamp(0.0, max_value) as u32,
            y: value.y.floor().clamp(0.0, max_value) as u32,
            zoom: value.zoom,
        }
    }
}

impl From<TilePosition> for TileCoordinates {
    fn from(value: TilePosition) -> Self {
        Self {
            x: value.x as f64,
            y: value.y as f64,
            zoom: value.zoom,
        }
    }
}

/// A frame around rectangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundingRectangle {
    pub x_min: u32,
    pub y_min: u32,
    pub width: u32,
    pub height: u32,
    pub zoom: u8,
}

/// Describe what tiles have changed.
#[derive(Debug, Clone, Default)]
pub struct TileChange {
    pub deleted: Vec<TilePosition>,
    pub added: Vec<TilePosition>,
}

/// The boundary latitude we do not overshoot.
const BOUNDARY_LATITUDE: f64 = 85.05112878;

/// The central request for a rectangle to subscribe to. We have a center point in tile coordinates and a
/// total width and height also in tile coordinates. This can be transfered into an optional bounding rectangle.
#[derive(Debug, Clone)]
pub struct RequestRectangle {
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub zoom: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum RectConversionError {
    NegativeSize,
    OutOfWorld,
}
impl TryFrom<&RequestRectangle> for BoundingRectangle {
    type Error = RectConversionError;

    fn try_from(rect: &RequestRectangle) -> Result<Self, Self::Error> {
        if (rect.width <= 0.0) || (rect.height <= 0.0) {
            return Err(RectConversionError::NegativeSize);
        }

        let max_value = ((1u64 << rect.zoom) - 1) as f32;

        let min_x = (rect.center_x - rect.width * 0.5).floor();
        let min_y = (rect.center_y - rect.height * 0.5).floor();
        let max_x = (rect.center_x + rect.width * 0.5).floor();
        let max_y = (rect.center_y + rect.height * 0.5).floor();

        // Check if we are totally empty.
        if max_x < 0.0 || max_y < 0.0 || min_x > max_value || min_y > max_value {
            return Err(RectConversionError::OutOfWorld);
        }

        let x_min_new = min_x.clamp(0.0, max_value) as u32;
        let y_min_new = min_y.clamp(0.0, max_value) as u32;
        let x_max_new = max_x.clamp(0.0, max_value) as u32;
        let y_max_new = max_y.clamp(0.0, max_value) as u32;

        Ok(BoundingRectangle {
            x_min: x_min_new,
            y_min: y_min_new,
            width: x_max_new - x_min_new + 1,
            height: y_max_new - y_min_new + 1,
            zoom: rect.zoom,
        })
    }
}

impl BoundingRectangle {
    /// Gets the bounding rectangle from a bunch of tile coordinates.
    pub fn new(positions: &[TilePosition]) -> Self {
        assert!(!positions.is_empty(), "We must contain some data");
        debug_assert!(
            positions.windows(2).all(|w| w[0].zoom == w[1].zoom),
            "All positions must share the same zoom level"
        );
        let (x_min, y_min, x_max, y_max) = positions.iter().inspect(|x| x.check_sanity()).fold(
            (u32::MAX, u32::MAX, 0, 0),
            |(x_min, y_min, x_max, y_max), tile| {
                (
                    tile.x.min(x_min),
                    tile.y.min(y_min),
                    tile.x.max(x_max),
                    tile.y.max(y_max),
                )
            },
        );

        Self {
            x_min,
            y_min,
            width: x_max - x_min + 1,
            height: y_max - y_min + 1,
            zoom: positions[0].zoom,
        }
    }

    /// Gets an iterator for the tile positions in that rectangle.
    pub fn get_iterator(&self) -> impl Iterator<Item = TilePosition> {
        iproduct!(0..self.width, 0..self.height)
            .map(move |(w, h)| TilePosition {
                x: self.x_min + w,
                y: self.y_min + h,
                zoom: self.zoom,
            })
            .inspect(|p| p.check_sanity())
    }

    /// Generates the bounding rectangle that include both.
    pub fn union(&self, other: &Self) -> Self {
        debug_assert!(self.zoom == other.zoom, "Zoom must be the same in union.");
        let x_min = self.x_min.min(other.x_min);
        let y_min = self.y_min.min(other.y_min);
        let x_max = (self.x_min + self.width - 1).max(other.x_min + other.width - 1);
        let y_max = (self.y_min + self.height - 1).max(other.y_min + other.height - 1);
        Self {
            x_min,
            y_min,
            width: x_max - x_min + 1,
            height: y_max - y_min + 1,
            zoom: self.zoom,
        }
    }

    /// Simply checks if we are in that position.
    pub fn contains_position(&self, coordinates: &TilePosition) -> bool {
        (self.zoom == coordinates.zoom)
            && (self.x_min..self.x_min + self.width).contains(&coordinates.x)
            && (self.y_min..self.y_min + self.height).contains(&coordinates.y)
    }

    /// Compares ourselves against a new rectangle and flags which positions have arrived and which have left.
    pub fn generate_deletion_creation_list(&self, new_rectangle: &BoundingRectangle) -> TileChange {
        // If they ara  on different zoom levels we must completely replace it.
        if self.zoom != new_rectangle.zoom {
            return TileChange {
                deleted: self.get_iterator().collect(),
                added: new_rectangle.get_iterator().collect(),
            };
        }

        let mut added = Vec::new();
        let mut deleted = Vec::new();
        let frame = self.union(new_rectangle);
        for position in frame.get_iterator() {
            let is_in_old = self.contains_position(&position);
            let is_in_new = new_rectangle.contains_position(&position);

            if is_in_old && !is_in_new {
                deleted.push(position);
            }
            if !is_in_old && is_in_new {
                added.push(position);
            }
        }
        TileChange { added, deleted }
    }
}

/// Conversion between zoom level and scaling factor.
fn get_scaling_factor(zoom: u8) -> f64 {
    2u32.pow(zoom as u32) as f64
}

/// The latitude longitude pair. Both are given in degrees.
#[derive(Debug, Clone, Copy)]
pub struct LatitudeLongitude {
    latitude: f64,
    longitude: f64,
}

impl LatitudeLongitude {
    /// Getter latitude.
    pub fn latitude(&self) -> f64 {
        self.latitude
    }

    /// Getter longitude.
    pub fn longitude(&self) -> f64 {
        self.longitude
    }

    /// Constructs the object and makes sure, that both coordinates are in the valid range
    /// (latitude: -BOUNDARY_LATITUDE .. BOUNDARY_LATITUDE, longitude: -180 .. 180)
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude: latitude.clamp(-BOUNDARY_LATITUDE, BOUNDARY_LATITUDE),
            longitude: longitude.clamp(-180.0, 180.0),
        }
    }

    /// Gets the tile coordinates in the indicated zoom level
    pub fn get_tile_coordinates(&self, zoom: u8) -> TileCoordinates {
        let scaling = get_scaling_factor(zoom);

        let x = (self.longitude + 180.0) / 360.0 * scaling;
        let angle = self.latitude * PI / 180.0;
        let y = (1.0 - f64::ln(f64::tan(angle) + 1.0 / f64::cos(angle)) / PI) * scaling * 0.5;

        TileCoordinates { x, y, zoom }
    }
}

impl From<TileCoordinates> for LatitudeLongitude {
    fn from(value: TileCoordinates) -> Self {
        let scaling = get_scaling_factor(value.zoom);
        let longitude = (value.x) / scaling * 360.0 - 180.0;
        let latitude = f64::atan(f64::sinh(PI - value.y / scaling * 2.0 * PI)) * 180.0 / PI;

        Self::new(latitude, longitude)
    }
}

/// Splits the scaling factor coming in in float to the zoom level to be asked for and the scaling
/// to be applied to the rendering.
pub fn split_scaling(input: f32) -> (u8, f32) {
    let rounded = input.clamp(0.0, MAXIMUM_ZOOM_LEVEL as f32).round();
    (rounded as u8, 2.0f32.powf(input - rounded))
}

/// Helper structure to convert coordinates into actual drawing positions
pub struct DrawingPositionConverter {
    /// The offset in x,y that needs to get added.
    central_offset: (f64, f64),
    /// The scaling factor that needs to get applied in combination with offset for transformation.
    transform_scaling: f64,
    /// The scaling we need to apply to out sprite to render it.
    render_scaling: f32,
    /// The zoom step we work on.
    zoom: u8,
    /// The boundary rectangle needed for quering tiles.
    bounding_rectangle: Option<BoundingRectangle>,
    /// The original position in latitude longitude.
    original_position: LatitudeLongitude,
    /// The original continues scaling factor.
    original_scaling: f32,
}

impl DrawingPositionConverter {
    pub fn new(
        central_position: &LatitudeLongitude,
        scaling_global: f32,
        drawing_rect: &Rectangle,
    ) -> Self {
        let (zoom, render_scaling) = split_scaling(scaling_global);
        let transform_scaling = (render_scaling * TILE_SIZE_PIXEL as f32) as f64;
        let tile_center = central_position.get_tile_coordinates(zoom);
        let rect_center = (
            (drawing_rect.x + drawing_rect.width * 0.5) as f64,
            (drawing_rect.y + drawing_rect.height * 0.5) as f64,
        );
        let central_offset = (
            rect_center.0 - (tile_center.x * transform_scaling),
            rect_center.1 - (tile_center.y * transform_scaling),
        );

        let width_new = drawing_rect.width as f64 / transform_scaling;
        let height_new = drawing_rect.height as f64 / transform_scaling;

        let inner_rectangle = BoundingRectangle::try_from(&RequestRectangle {
            center_x: tile_center.x as f32,
            center_y: tile_center.y as f32,
            width: width_new as f32,
            height: height_new as f32,
            zoom,
        });

        debug_assert!(
            !matches!(inner_rectangle, Err(RectConversionError::NegativeSize)),
            "Negative size should not happen in calculation."
        );

        Self {
            central_offset,
            transform_scaling,
            render_scaling,
            zoom,
            bounding_rectangle: inner_rectangle.ok(),
            original_position: *central_position,
            original_scaling: scaling_global,

        }
    }

    /// Gets the original position.
    pub fn original_position(&self) -> LatitudeLongitude {self.original_position}

    /// Asks for the original scaling.
    pub fn original_scaling(&self) -> f32 {self.original_scaling}
    /// Asks for the bounding retangle if existing.
    pub fn bounding_rectangle(&self) -> Option<BoundingRectangle> {
        self.bounding_rectangle
    }

    /// Asks for the scaling that has to be applied when rendering a tile.
    pub fn get_drawing_scale(&self) -> f32 {
        self.render_scaling
    }

    /// Gets the drawin position of a tile handed over returns a vector from iced.
    pub fn get_drawing_position(&self, tile_pos: TileCoordinates) -> Vector {
        debug_assert_eq!(tile_pos.zoom, self.zoom, "Zoom level incompatible.");
        Vector::new
        (
            (tile_pos.x * self.transform_scaling + self.central_offset.0) as f32,
            (tile_pos.y * self.transform_scaling + self.central_offset.1) as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Point, Size};
    use proptest::{prop_assert, proptest};

    proptest! {
        #[test]
        fn drawing_position_converter(latitude in -90f64 .. 90f64, longitude in -180f64 .. 180f64,
            zoom in 0u8 ..=MAXIMUM_ZOOM_LEVEL, width in 1f32..1000.0, height in 1f32..1000.0) {

            let bounding_rect = Rectangle::new(Point{x: 0.0, y: 0.0}, Size {width, height});
            let compound_zoom = zoom as f32;
            let focus_point = LatitudeLongitude::new(latitude, longitude);

            let transformer = DrawingPositionConverter::new(&focus_point, compound_zoom, &bounding_rect);

            prop_assert!((1.0 - transformer.get_drawing_scale()).abs() < 1e-5, "There should be no scale.");
            let drawing = transformer.get_drawing_position(focus_point.get_tile_coordinates(zoom));
            prop_assert!((width * 0.5 - drawing.x).abs() < 0.01, "x coordinate off" );
            prop_assert!((height * 0.5 - drawing.y).abs() < 0.01, "x coordinate off" );
        }
    }


    #[test]
    fn boundary_test() {
        let coord = LatitudeLongitude::from(TileCoordinates {
            x: 0.0,
            y: 0.0,
            zoom: 0,
        });
        assert!(f64::abs(coord.latitude() - BOUNDARY_LATITUDE) < 1e-9);
    }

    #[test]
    fn creation_test() {
        let rect = BoundingRectangle::new(&[TilePosition {
            x: 0,
            y: 0,
            zoom: 0,
        }]);
        assert_eq!(rect.width, 1);
        assert_eq!(rect.height, 1);
        assert_eq!(rect.get_iterator().count(), 1);
    }

    #[test]
    fn square_test() {
        let rect = BoundingRectangle::new(&[
            TilePosition {
                x: 0,
                y: 0,
                zoom: 2,
            },
            TilePosition {
                x: 1,
                y: 1,
                zoom: 2,
            },
            TilePosition {
                x: 2,
                y: 2,
                zoom: 2,
            },
        ]);
        assert_eq!(rect.width, 3);
        assert_eq!(rect.height, 3);
        assert_eq!(rect.get_iterator().count(), 9);
    }

    #[test]
    fn change_test() {
        let first_rect = BoundingRectangle::new(&[
            TilePosition {
                x: 0,
                y: 0,
                zoom: 2,
            },
            TilePosition {
                x: 1,
                y: 1,
                zoom: 2,
            },
        ]);
        let change = first_rect.generate_deletion_creation_list(&first_rect);
        assert!(change.added.is_empty());
        assert!(change.deleted.is_empty());
        let second_rect = BoundingRectangle::new(&[
            TilePosition {
                x: 1,
                y: 1,
                zoom: 2,
            },
            TilePosition {
                x: 2,
                y: 2,
                zoom: 2,
            },
        ]);
        let change = first_rect.generate_deletion_creation_list(&second_rect);
        assert_eq!(change.added.len(), 3);
        assert_eq!(change.deleted.len(), 3);

        let first_tile = TilePosition {
            x: 0,
            y: 0,
            zoom: 1,
        };
        let second_tile = TilePosition {
            x: 1,
            y: 1,
            zoom: 1,
        };
        let simple_a = BoundingRectangle::new(&[first_tile]);
        let simple_b = BoundingRectangle::new(&[second_tile]);
        let change = simple_a.generate_deletion_creation_list(&simple_b);
        assert_eq!(change.deleted, vec![first_tile]);
        assert_eq!(change.added, vec![second_tile]);
    }

    proptest! {
        #[test]
        fn coordinate_test(latitude in -90f64 .. 90f64, longitude in -180f64 .. 180f64, zoom in 0u8 ..=MAXIMUM_ZOOM_LEVEL) {
            let orig_pos = LatitudeLongitude::new(latitude, longitude);
            let tile = orig_pos.get_tile_coordinates(zoom);
            let new_pos :LatitudeLongitude = tile.into();

            prop_assert!( f64::abs(new_pos.longitude -  orig_pos.longitude) < 1e-5);
            prop_assert!( f64::abs(new_pos.latitude -  orig_pos.latitude) < 1e-5);

        }
    }
}
