//! This module contains all related to math and coordinates.

use itertools::iproduct;

/// The tile coordinates in float space,
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TileCoordinates {
    pub x: f32,
    pub y: f32,
    pub zoom: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TilePosition {
    pub x: u32,
    pub y: u32,
    pub zoom: u8,
}

impl From<TileCoordinates> for TilePosition {
    fn from(value: TileCoordinates) -> Self {
        debug_assert!(
            value.x >= 0.0 && value.y >= 0.0,
            "Only positive coordinates are allowed"
        );
        Self {
            x: value.x.floor() as u32,
            y: value.y.floor() as u32,
            zoom: value.zoom,
        }
    }
}

/// A frame around rectangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BoundingRectangle {
    pub x_min: u32,
    pub y_min: u32,
    pub width: u32,
    pub height: u32,
}

/// Describe what tiles have changed.
#[derive(Debug, Clone)]
pub struct TileChange {
    pub deleted: Vec<TilePosition>,
    pub added: Vec<TilePosition>,
}

impl BoundingRectangle {
    /// Gets the bounding rectangle from a bunch of tile coordinates.
    pub fn new(positions: &[TilePosition]) -> Self {
        debug_assert!(!positions.is_empty(), "We must contain some data");
        let (x_min, y_min, x_max, y_max) = positions.iter().fold(
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
        }
    }

    /// Gets an iterator for the tile positions in that rectangle.
    pub fn get_iterator(&self, zoom: u8) -> impl Iterator<Item = TilePosition> {
        iproduct!(0..self.width, 0..self.height).map(move |(w, h)| TilePosition {
            x: self.x_min + w,
            y: self.y_min + h,
            zoom,
        })
    }

    /// Generates the bounding rectangle that include both.
    pub fn union(&self, other: &Self) -> Self {
        let x_min = self.x_min.min(other.x_min);
        let y_min = self.y_min.min(other.y_min);
        let x_max = (self.x_min + self.width - 1).max(other.x_min + other.width - 1);
        let y_max = (self.y_min + self.height - 1).max(other.y_min + other.height - 1);
        Self {
            x_min,
            y_min,
            width: x_max - x_min + 1,
            height: y_max - y_min + 1,
        }
    }

    /// Simply checks if we are in that position.
    pub fn contains_position(&self, coordinates: &TilePosition) -> bool {
        (self.x_min..self.x_min + self.width).contains(&coordinates.x)
            && (self.y_min..self.y_min + self.height).contains(&coordinates.y)
    }

    /// Compares ourselves against a new rectangle and flags which positions have arrived and which have left.
    pub fn generate_deletion_creation_list(
        &self,
        new_rectangle: &BoundingRectangle,
        zoom: u8,
    ) -> TileChange {
        let mut added = Vec::new();
        let mut deleted = Vec::new();
        let frame = self.union(new_rectangle);
        for position in frame.get_iterator(zoom) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_test() {
        let rect = BoundingRectangle::new(&[TilePosition {
            x: 0,
            y: 0,
            zoom: 0,
        }]);
        assert_eq!(rect.width, 1);
        assert_eq!(rect.height, 1);
        assert_eq!(rect.get_iterator(0).count(), 1);
    }

    #[test]
    fn square_test() {
        let rect = BoundingRectangle::new(&[
            TilePosition {
                x: 0,
                y: 0,
                zoom: 0,
            },
            TilePosition {
                x: 1,
                y: 1,
                zoom: 0,
            },
            TilePosition {
                x: 2,
                y: 2,
                zoom: 0,
            },
        ]);
        assert_eq!(rect.width, 3);
        assert_eq!(rect.height, 3);
        assert_eq!(rect.get_iterator(0).count(), 9);
    }

    #[test]
    fn change_test() {
        let first_rect = BoundingRectangle::new(&[
            TilePosition {
                x: 0,
                y: 0,
                zoom: 0,
            },
            TilePosition {
                x: 1,
                y: 1,
                zoom: 0,
            },
        ]);
        let change = first_rect.generate_deletion_creation_list(&first_rect, 0);
        assert!(change.added.is_empty());
        assert!(change.deleted.is_empty());
        let second_rect = BoundingRectangle::new(&[
            TilePosition {
                x: 1,
                y: 1,
                zoom: 0,
            },
            TilePosition {
                x: 2,
                y: 2,
                zoom: 0,
            },
        ]);
        let change = first_rect.generate_deletion_creation_list(&second_rect, 0);
        assert_eq!(change.added.len(), 3);
        assert_eq!(change.deleted.len(), 3);

        let first_tile = TilePosition {
            x: 0,
            y: 0,
            zoom: 0,
        };
        let second_tile = TilePosition {
            x: 1,
            y: 1,
            zoom: 0,
        };
        let simpa = BoundingRectangle::new(&[first_tile]);
        let simpb = BoundingRectangle::new(&[second_tile]);
        let change = simpa.generate_deletion_creation_list(&simpb, 0);
        assert_eq!(change.deleted, vec![first_tile]);
        assert_eq!(change.added, vec![second_tile]);
    }
}
