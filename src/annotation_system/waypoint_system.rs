//! This module administrates the way points, A way point is a single isolated location on the map.
//! This differentiates from the paths, where several points are interconnected with each other.

use bytes::Bytes;
use iced::advanced::image::Handle;
use iced::Color;
use crate::gui_system::internal_math::{BoundingRectangle, LatitudeLongitude, TilePosition};
use slotmap::{DefaultKey, SlotMap};


/// The way point parameter symbol we paint over.
pub enum WaypointSymbol {
    /// Image with raw image data, from an image file.
    Image(Bytes),
    /// A cross we want to draw with an indicated color.
    Cross(Color),
}


/// The administration for the whole waypoint system.
#[derive(Debug, Clone, Default)]
pub struct WaypointSystem {
    /// The collection as a slot map.
    waypoint_collection : SlotMap<DefaultKey, WaypointInfo>
}

impl WaypointSystem {

    /// Adds a new waypoint to the system.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::internal_math::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// ```
    pub fn add_way_point(&mut self, symbol: WaypointSymbol, position: LatitudeLongitude, description: Option<String>) -> DefaultKey{
        let image = match symbol {
            WaypointSymbol::Image(image) => InternalWaypointImage::Image(Handle::from_bytes(image)),
            WaypointSymbol::Cross(color) => InternalWaypointImage::Cross(color)
        };

        let waypoint = WaypointInfo {
            image,
            position,
            description
        };
        self.waypoint_collection.insert(waypoint)
    }

    /// Deletes the way point with the indicated key and returns it.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::internal_math::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// system.delete_waypoint(key);
    /// ```
    pub fn delete_waypoint(&mut self, key: DefaultKey) -> Option<WaypointInfo> {
        self.waypoint_collection.remove(key)
    }

    /// Gets a reference to the waypoint info if existing.
    ///
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::internal_math::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let point = system.get_waypoint_info(key);
    /// ```
    pub fn get_waypoint_info(&self, key: DefaultKey) -> Option<&WaypointInfo> {
        self.waypoint_collection.get(key)
    }

    /// Gets all relevant items waypoint for the indicated bounding rectangle.
    /// Mainly intended for internal rendering.
    pub(crate) fn get_all_relevant_waypoints(&self, area: &BoundingRectangle) -> Vec<WaypointInfo>   {
        let zoom = area.zoom;
        self.waypoint_collection.values().filter_map( move |point| {
            let tile_pos = point.position.get_tile_coordinates(zoom);
            area.contains_position(&TilePosition::from(tile_pos)).then_some(point.clone())
        }).collect()
    }
}

/// The internal way point image we have as a way point.
#[derive(Debug, Clone)]
enum InternalWaypointImage{
    /// A registered image as a handle.
    Image(Handle),
    /// A cross with an indicated color.
    Cross(Color),
}

/// The way point information stored.
#[derive(Debug, Clone)]
pub struct WaypointInfo {
    /// This contains the graphical representation.
    pub image: InternalWaypointImage,
    /// The position on the map.
    pub position: LatitudeLongitude,
    /// An optional string that may be drawn in hover over.
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn creation_deletion() {
        let mut system = WaypointSystem::default();

        let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
        assert_ne!(key, DefaultKey::default());
        let first = system.delete_waypoint(key);
        assert!(first.is_some(), "We should have some data here.");
        let second = system.delete_waypoint(key);
        assert!(second.is_none(), "That should be empty.");

    }
}