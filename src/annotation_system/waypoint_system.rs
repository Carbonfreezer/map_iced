//! This module administrates the way points, A way point is a single isolated location on the map.
//! This differentiates from the paths, where several points are interconnected with each other.

use bytes::Bytes;
use iced::advanced::image::Handle;
use iced::Color;
use crate::gui_system::internal_math::{BoundingRectangle, LatitudeLongitude, TilePosition};
use slotmap::{new_key_type, SlotMap};

new_key_type! {
    /// The handle of a way point inside a [`WaypointSystem`]. A type of its own, so
    /// that it cannot be confused with the handles of the other annotation kinds.
    pub struct WaypointKey;
}


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
    waypoint_collection : SlotMap<WaypointKey, WaypointInfo>,
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
    pub fn add_way_point(&mut self, symbol: WaypointSymbol, position: LatitudeLongitude, description: Option<String>) -> WaypointKey{
        let image = match symbol {
            WaypointSymbol::Image(image) => InternalWaypointImage::Image(Handle::from_bytes(image)),
            WaypointSymbol::Cross(color) => InternalWaypointImage::Cross(color)
        };

        self.waypoint_collection.insert_with_key(|key| WaypointInfo {
            key,
            image,
            position,
            description,
            flag: None,
        })
    }

    /// Flags the way point, or takes the flag away with `None`. A flagged way point
    /// gets an arrow on the edge of every widget it is off screen in. Returns
    /// `false` for an unknown key.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointFlag, WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::internal_math::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let flag = WaypointFlag { color: Color::from_rgb(1.0, 0.0, 0.0), priority: 3 };
    /// assert!(system.set_flag(key, Some(flag)));
    /// ```
    pub fn set_flag(&mut self, key: WaypointKey, flag: Option<WaypointFlag>) -> bool {
        match self.waypoint_collection.get_mut(key) {
            Some(point) => {
                point.flag = flag;
                true
            }
            None => false,
        }
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
    pub fn delete_waypoint(&mut self, key: WaypointKey) -> Option<WaypointInfo> {
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
    pub fn get_waypoint_info(&self, key: WaypointKey) -> Option<&WaypointInfo> {
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

    /// All flagged way points, wherever they are. Deliberately not filtered by area:
    /// the arrows are needed for exactly those points that are off screen.
    pub(crate) fn get_all_flagged_waypoints(&self) -> Vec<WaypointInfo> {
        self.waypoint_collection
            .values()
            .filter(|point| point.flag.is_some())
            .cloned()
            .collect()
    }
}

/// The internal way point image we have as a way point.
#[derive(Debug, Clone)]
pub(crate) enum InternalWaypointImage{
    /// A registered image as a handle.
    Image(Handle),
    /// A cross with an indicated color.
    Cross(Color),
}

/// The way point information stored.
#[derive(Debug, Clone)]
pub struct WaypointInfo {
    /// The key this way point is stored under. Stable across focal point changes,
    /// unlike the index into a way point snapshot.
    pub key: WaypointKey,
    /// This contains the graphical representation. Internal, the outside has no
    /// business with the way we hold on to an image or a colour.
    pub(crate) image: InternalWaypointImage,
    /// The position on the map.
    pub position: LatitudeLongitude,
    /// An optional string that may be drawn in hover over.
    pub description: Option<String>,
    /// The direction arrow, if the way point is flagged. Set with
    /// [`WaypointSystem::set_flag`].
    pub flag: Option<WaypointFlag>,
}

/// How the direction arrow of a flagged way point looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaypointFlag {
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
    fn creation_deletion() {
        let mut system = WaypointSystem::default();

        let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
        assert_ne!(key, WaypointKey::default());
        let first = system.delete_waypoint(key);
        assert!(first.is_some(), "We should have some data here.");
        let second = system.delete_waypoint(key);
        assert!(second.is_none(), "That should be empty.");

    }
}
