//! This module contains all related to latitude longitude
//! and tile coordinate system. This is the pure internal representation
//! of the coordinates. It contains all to the tile coordinates used in the
//! slippy tile system. It also administrates badges (bounding rectangles of tiles).
//! Can be useful for internal computations.

use crate::annotation_system::annotation_support::{MercatorRectangle, TopLeftBottomRight};
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use iced::{Point, Rectangle, Size, Vector};
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
/// These are the coordinates that go into the slippy
/// tile system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TilePosition {
    pub x: u32,
    pub y: u32,
    pub zoom: u8,
}

impl TilePosition {
    /// Pure debug function. Checks if the tile position is legal with a series
    /// of debug asserts
    pub fn check_sanity(&self) {
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
    /// Conversion - along the way we clamp to the legal range.
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

/// An enclosing rectangle for tiles at a certain zoom level using tile indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundingRectangle {
    pub x_min: u32,
    pub y_min: u32,
    pub width: u32,
    pub height: u32,
    pub zoom: u8,
}

/// Describes what tiles have changed.
#[derive(Debug, Clone, Default)]
pub struct TileChange {
    pub deleted: Vec<TilePosition>,
    pub added: Vec<TilePosition>,
}

/// The central request for a rectangle to subscribe to. We have a center point in tile coordinates and a
/// total width and height also in tile coordinates. This can be transferred into an optional bounding rectangle.
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
    // TODO: Currently not used check for usage later on.
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
pub fn get_scaling_factor(zoom: u8) -> f64 {
    f64::exp2(zoom as f64)
}

impl From<TileCoordinates> for LatitudeLongitude {
    fn from(value: TileCoordinates) -> Self {
        let scaling = get_scaling_factor(value.zoom);
        let longitude = (value.x) / scaling * 360.0 - 180.0;
        let latitude = f64::atan(f64::sinh(PI - value.y / scaling * 2.0 * PI)) * 180.0 / PI;

        LatitudeLongitude::new(latitude, longitude)
    }
}

/// Splits the scaling factor coming in in float to the zoom level to be asked for and the scaling
/// to be applied to the rendering.
pub fn split_scaling(input: f32) -> (u8, f32) {
    let rounded = input.clamp(0.0, MAXIMUM_ZOOM_LEVEL as f32).round();
    (rounded as u8, f32::exp2(input - rounded))
}

/// Everything needed to render one tile sprite into the current view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileDrawInstruction {
    /// Translation of the tile's top left corner in widget coordinates.
    pub offset: Vector,
    /// Uniform scale applied to the 256x256 sprite after translating.
    pub scale: f32,
}

/// Helper structure to convert coordinates into actual drawing positions.
/// This depends on the current center of the drawing area and the zoom factor applied.
pub struct DrawingPositionConverter {
    /// The offset in x,y that needs to get added.
    central_offset: Vector<f64>,
    /// The center of the tile in tile coordinates.
    tile_center: TileCoordinates,
    /// The scaling factor that needs to get applied in combination with offset for transformation.
    transform_scaling: f64,
    /// The scaling we need to apply to out sprite to render it.
    render_scaling: f32,
    /// Size of the drawing area in pixels, needed to cull tiles of any zoom level.
    drawing_size: Size<f64>,
}

impl DrawingPositionConverter {
    pub fn new(
        central_position: &LatitudeLongitude,
        scaling_global: f32,
        drawing_rect: &Rectangle,
    ) -> (Self, Result<BoundingRectangle, RectConversionError>) {
        let (zoom, render_scaling) = split_scaling(scaling_global);
        let transform_scaling = (render_scaling * TILE_SIZE_PIXEL as f32) as f64;
        let tile_center = central_position.get_tile_coordinates(zoom);
        let rect_center = (
            (drawing_rect.width * 0.5) as f64,
            (drawing_rect.height * 0.5) as f64,
        );
        let central_offset = Vector::new(
            rect_center.0 - (tile_center.x * transform_scaling),
            rect_center.1 - (tile_center.y * transform_scaling),
        );
        let drawing_size = Size::new(drawing_rect.width as f64, drawing_rect.height as f64);

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

        (
            Self {
                tile_center,
                central_offset,
                transform_scaling,
                render_scaling,
                drawing_size,
            },
            inner_rectangle,
        )
    }

    /// Size of the drawing area in pixels.
    pub(crate) fn drawing_size(&self) -> Size {
        Size::new(
            self.drawing_size.width as f32,
            self.drawing_size.height as f32,
        )
    }

    /// Gets the discreet zoom level.
    pub fn zoom(&self) -> u8 {
        self.tile_center.zoom
    }

    /// Draw instruction for a tile of *any* zoom level. `None` when the tile
    /// cannot contribute a pixel to the current viewport.
    pub fn get_draw_instruction(&self, tile: TileCoordinates) -> Option<TileDrawInstruction> {
        debug_assert!(tile.zoom <= MAXIMUM_ZOOM_LEVEL, "zoom out of range");

        let factor = f64::exp2(self.tile_center.zoom as f64 - tile.zoom as f64);
        let extent = factor * self.transform_scaling; // on-screen edge length

        let x = tile.x * extent + self.central_offset.x;
        let y = tile.y * extent + self.central_offset.y;

        if x + extent <= 0.0
            || y + extent <= 0.0
            || x >= self.drawing_size.width
            || y >= self.drawing_size.height
        {
            return None;
        }

        Some(TileDrawInstruction {
            offset: Vector::new(x as f32, y as f32),
            scale: (self.render_scaling as f64 * factor) as f32,
        })
    }

    /// Gets the drawing position from within the widget for a certain Latitude, Longitude Vector.
    /// Test for visibility is already done upfront when changing the focus for the culling bounds.
    pub fn get_drawing_position(&self, pos: LatitudeLongitude) -> Vector {
        let Vector { x, y } = self.get_unclipped_drawing_position(pos);

        Vector::new(x as f32, y as f32)
    }

    /// The position within the widget for a certain Latitude, Longitude, also when
    /// it lies far outside. Kept in `f64`, at high zoom a distant point is millions
    /// of pixels away.
    pub fn get_unclipped_drawing_position(&self, pos: LatitudeLongitude) -> Vector<f64> {
        let tile = pos.get_tile_coordinates(self.zoom());
        Vector::new(
            tile.x * self.transform_scaling + self.central_offset.x,
            tile.y * self.transform_scaling + self.central_offset.y,
        )
    }

    /// Focus that results from dragging the map by `delta` pixels.
    pub fn get_new_coord_for_mouse_delta(&self, delta: Vector) -> LatitudeLongitude {
        let shifted = TileCoordinates {
            x: self.tile_center.x - delta.x as f64 / self.transform_scaling,
            y: self.tile_center.y - delta.y as f64 / self.transform_scaling,
            zoom: self.tile_center.zoom,
        };
        shifted.into()
    }

    /// Computes the latitude and longitude for a mouse coordinate in the widget handed over.
    pub fn get_latitude_longitude_for_pixel_point(&self, position: Point) -> LatitudeLongitude {
        let shifted = TileCoordinates {
            x: (position.x as f64 - self.central_offset.x) / self.transform_scaling,
            y: (position.y as f64 - self.central_offset.y) / self.transform_scaling,
            zoom: self.tile_center.zoom,
        };
        shifted.into()
    }

    /// Checks of a mercator rectangle handed over is actually visible by the inner drawing rectangle.
    pub fn is_mercator_visible(&self, mercator_rect: &MercatorRectangle) -> bool {
        // If there is no real rectangle it may never become visible.
        if !mercator_rect.is_valid {
            return false;
        };
        let boundary = TopLeftBottomRight::from(mercator_rect);
        let top_left = self.get_unclipped_drawing_position(boundary.top_left);
        let bottom_right = self.get_unclipped_drawing_position(boundary.bottom_right);
        // Drawing positions are relative to the widget, so the test runs against
        // 0..drawing_size, not against the widget bounds in the window. Kept in f64,
        // at high zoom the corners of a long track are millions of pixels away.
        let padding = mercator_rect.pixel_padding;
        top_left.x - padding < self.drawing_size.width
            && top_left.y - padding < self.drawing_size.height
            && bottom_right.x + padding > 0.0
            && bottom_right.y + padding > 0.0
    }

    /// Analyzes whether a drawing position handed over is in the mercator rectangle.
    /// This method is intended as a precheck for the hit point test on drawing elements.
    /// Warning: point must be in widget relative coordinates.
    pub fn is_pixel_point_in_mercator(
        &self,
        point: Point,
        mercator_rect: &MercatorRectangle,
    ) -> bool {
        if !mercator_rect.is_valid {
            return false;
        };
        let boundary = TopLeftBottomRight::from(mercator_rect);
        let top_left = self.get_unclipped_drawing_position(boundary.top_left);
        let bottom_right = self.get_unclipped_drawing_position(boundary.bottom_right);
        let padding = mercator_rect.pixel_padding;

        point.x as f64 >= top_left.x - padding
            && point.x as f64 <= bottom_right.x + padding
            && point.y as f64 >= top_left.y - padding
            && point.y as f64 <= bottom_right.y + padding
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui_system::latitude_longitude::BOUNDARY_LATITUDE;
    use iced::{Point, Size};
    use proptest::{prop_assert, prop_assert_eq, prop_assume, proptest};

    proptest! {
        /// The rectangle is spanned by two widget pixels that are turned into latitude longitude,
        /// so the pixel rectangle they span is the independent oracle. Focus and zoom keep the widget
        /// inside the world, where nothing gets clamped. The point may lie far outside the widget.
        #[test]
        fn pixel_point_in_rectangle(latitude in -60f64 .. 60f64, longitude in -150f64 .. 150f64,
            scaling in 8f32 ..= MAXIMUM_ZOOM_LEVEL as f32, width in 1f32..1000.0, height in 1f32..1000.0,
            corner_a in (0f32..1.0, 0f32..1.0), corner_b in (0f32..1.0, 0f32..1.0),
            padding in 0f64..50.0, point in (-0.5f32..1.5, -0.5f32..1.5)) {

            let bounds = Rectangle::new(Point { x: 0.0, y: 0.0 }, Size { width, height });
            let focus = LatitudeLongitude::new(latitude, longitude);
            let converter = DrawingPositionConverter::new(&focus, scaling, &bounds).0;

            let corners = [corner_a, corner_b].map(|(x, y)| Point { x: x * width, y: y * height });
            let rect = MercatorRectangle::create_from_position_array(
                corners.iter().map(|p| converter.get_latitude_longitude_for_pixel_point(*p)),
                padding,
            );
            let point = Point { x: point.0 * width, y: point.1 * height };

            let (x, y) = (point.x as f64, point.y as f64);
            let x_range = (corners[0].x.min(corners[1].x) as f64 - padding, corners[0].x.max(corners[1].x) as f64 + padding);
            let y_range = (corners[0].y.min(corners[1].y) as f64 - padding, corners[0].y.max(corners[1].y) as f64 + padding);
            // The round trip through latitude longitude is not exact, stay clear of the edges.
            const MARGIN: f64 = 1e-3;
            prop_assume!([x - x_range.0, x - x_range.1, y - y_range.0, y - y_range.1].iter().all(|d| d.abs() > MARGIN));

            let expected = (x_range.0..=x_range.1).contains(&x) && (y_range.0..=y_range.1).contains(&y);
            prop_assert_eq!(converter.is_pixel_point_in_mercator(point, &rect), expected);
        }
    }

    /// A single position as used by the way points: the padding alone spans the hit area,
    /// and an invalid rectangle never gets hit.
    #[test]
    fn pixel_point_in_waypoint_rectangle() {
        let bounds = Rectangle::new(Point { x: 800.0, y: 600.0 }, Size::new(400.0, 300.0));
        let focus = LatitudeLongitude::new(50.0, 8.0);
        let converter = DrawingPositionConverter::new(&focus, 12.0, &bounds).0;
        let center = Point { x: 200.0, y: 150.0 };
        let waypoint = MercatorRectangle::create_from_position(focus, 10.0);

        assert!(converter.is_pixel_point_in_mercator(center, &waypoint));
        assert!(converter.is_pixel_point_in_mercator(Point { x: 209.0, y: 141.0 }, &waypoint));
        assert!(!converter.is_pixel_point_in_mercator(Point { x: 211.0, y: 150.0 }, &waypoint));
        assert!(!converter.is_pixel_point_in_mercator(Point { x: 200.0, y: 139.0 }, &waypoint));
        // Window coordinates are not widget coordinates.
        assert!(!converter.is_pixel_point_in_mercator(
            Point {
                x: 1000.0,
                y: 750.0
            },
            &waypoint
        ));

        let empty = MercatorRectangle::create_from_position_array(std::iter::empty(), 1e9);
        assert!(!converter.is_pixel_point_in_mercator(center, &empty));
    }

    proptest! {
        #[test]
        fn drawing_position_converter(latitude in -90f64 .. 90f64, longitude in -180f64 .. 180f64,
            zoom in 0u8 ..=MAXIMUM_ZOOM_LEVEL, width in 1f32..1000.0, height in 1f32..1000.0) {

            let bounding_rect = Rectangle::new(Point{x: 0.0, y: 0.0}, Size {width, height});
            let compound_zoom = zoom as f32;
            let focus_point = LatitudeLongitude::new(latitude, longitude);

            let transformer = DrawingPositionConverter::new(&focus_point, compound_zoom, &bounding_rect).0;

            let drawing = transformer.get_draw_instruction(focus_point.get_tile_coordinates(zoom)).expect("Zoom level should fit.");
            prop_assert!((width * 0.5 - drawing.offset.x).abs() < 0.01, "x coordinate off" );
            prop_assert!((height * 0.5 - drawing.offset.y).abs() < 0.01, "x coordinate off" );
        }
    }

    /// A widget that does not sit at the window origin, as the second of several
    /// map widgets would. Visibility must only depend on its size.
    #[test]
    fn mercator_visibility_ignores_widget_offset() {
        let bounds = Rectangle::new(Point { x: 800.0, y: 600.0 }, Size::new(400.0, 300.0));
        let focus = LatitudeLongitude::new(50.0, 8.0);
        let converter = DrawingPositionConverter::new(&focus, 12.0, &bounds).0;
        let at_pixel =
            |x: f32, y: f32| converter.get_latitude_longitude_for_pixel_point(Point { x, y });
        let point_rect =
            |position, padding| MercatorRectangle::create_from_position(position, padding);

        assert!(converter.is_mercator_visible(&point_rect(focus, 0.0)));
        assert!(converter.is_mercator_visible(&point_rect(at_pixel(5.0, 5.0), 0.0)));
        // Left of the widget, where the old test against the window bounds failed.
        assert!(!converter.is_mercator_visible(&point_rect(at_pixel(-20.0, 150.0), 0.0)));
        assert!(converter.is_mercator_visible(&point_rect(at_pixel(-20.0, 150.0), 30.0)));
        // A track crossing the whole view with both ends far outside.
        let track = MercatorRectangle::create_from_position_array(
            [at_pixel(-1e6, 150.0), at_pixel(1e6, 160.0)].into_iter(),
            0.0,
        );
        assert!(converter.is_mercator_visible(&track));
        let below = MercatorRectangle::create_from_position_array(
            [at_pixel(-1e6, 400.0), at_pixel(1e6, 420.0)].into_iter(),
            0.0,
        );
        assert!(!converter.is_mercator_visible(&below));
    }

    #[test]
    fn boundary_test() {
        let coord = LatitudeLongitude::from(TileCoordinates {
            x: 0.0,
            y: 0.0,
            zoom: 0,
        });
        assert!(f64::abs(coord.latitude - BOUNDARY_LATITUDE) < 1e-9);
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
