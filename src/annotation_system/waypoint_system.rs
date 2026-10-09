//! This module administrates the way points, A way point is a single isolated location on the map.
//! This differentiates from the paths, where several points are interconnected with each other.

use crate::gui_system::latitude_longitude::LatitudeLongitude;
use bytes::Bytes;
use iced::Color;
use iced::advanced::image::Handle;
use slotmap::{SlotMap, new_key_type};
use crate::annotation_system::annotation_support::{Annotation, LabelPosition, MercatorRectangle, RenderingInformation};
use crate::gui_system::map_widget::map_widget_core::WAYPOINT_HALF_SIZE;

// TODO: All indizes have to get into an enum
new_key_type! {
    /// The handle of a way point inside a [`WaypointSystem`]. A type of its own, so
    /// that it cannot be confused with the handles of the other annotation kinds.
    pub struct WaypointKey;
}

/// The way point parameter symbol we paint over the map.
pub enum WaypointSymbol {
    /// Image with raw image data, from an image file.
    Image(Bytes),
    /// A cross we want to draw with an indicated color.
    Cross(Color),
}

// TODO: accumulate this in three separate annotation systems. Unifying it over an enum is not worth the huzzle.

/// The administration for the whole waypoint system.
#[derive(Debug, Clone, Default)]
pub struct WaypointSystem {
    /// The collection as a slot map.
    pub(crate) waypoint_collection: SlotMap<WaypointKey, WaypointInfo>,
}

/// Error type for that case that the waypoint is not contained in the structure.
#[derive(Debug, Clone, Copy)]
pub struct WaypointKeyNotContained;

impl WaypointSystem {
    /// Adds a new waypoint to the system.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// ```
    pub fn add_way_point(
        &mut self,
        symbol: WaypointSymbol,
        position: LatitudeLongitude,
        description: Option<String>,
    ) -> WaypointKey {
        let image = match symbol {
            WaypointSymbol::Image(image) => InternalWaypointImage::Image(Handle::from_bytes(image)),
            WaypointSymbol::Cross(color) => InternalWaypointImage::Cross(color),
        };

        self.waypoint_collection
            .insert( WaypointInfo {
                image,
                mercator_rectangle: MercatorRectangle::create_from_position(position, WAYPOINT_HALF_SIZE as f64),
                description,
                flag: None,
            })
    }

    /// Sets the position of an existing way point.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.update_waypoint_position(key, LatitudeLongitude::new(50.0, 8.0));
    /// ```
    pub fn update_waypoint_position(
        &mut self,
        key: WaypointKey,
        position: LatitudeLongitude,
    ) -> Result<(), WaypointKeyNotContained> {
        self.waypoint_collection
            .get_mut(key)
            .ok_or(WaypointKeyNotContained)?
            .mercator_rectangle = MercatorRectangle::create_from_position(position, WAYPOINT_HALF_SIZE as f64);
        Ok(())
    }

    /// Sets the image / symbol of an existing way point.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.update_waypoint_symbol(key, WaypointSymbol::Cross(Color::BLACK));
    /// ```
    pub fn update_waypoint_symbol(
        &mut self,
        key: WaypointKey,
        symbol: WaypointSymbol,
    ) -> Result<(), WaypointKeyNotContained> {
        let image = match symbol {
            WaypointSymbol::Image(image) => InternalWaypointImage::Image(Handle::from_bytes(image)),
            WaypointSymbol::Cross(color) => InternalWaypointImage::Cross(color),
        };
        self.waypoint_collection
            .get_mut(key)
            .ok_or(WaypointKeyNotContained)?
            .image = image;
        Ok(())
    }

    /// Sets the (optional) of an existing way point.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.update_waypoint_description(key, Some("Annotation".to_string()));
    /// ```
    pub fn update_waypoint_description(
        &mut self,
        key: WaypointKey,
        description: Option<String>,
    ) -> Result<(), WaypointKeyNotContained> {
        self.waypoint_collection
            .get_mut(key)
            .ok_or(WaypointKeyNotContained)?
            .description = description;
        Ok(())
    }

    /// Flags the way point, or takes the flag away with `None`. A flagged way point
    /// gets an arrow on the edge of every widget it is off screen in. Returns
    /// `false` for an unknown key.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{DirectiontFlag, WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let flag = DirectiontFlag { color: Color::from_rgb(1.0, 0.0, 0.0), priority: 3 };
    /// assert!(system.set_flag(key, Some(flag)));
    /// ```
    pub fn set_flag(&mut self, key: WaypointKey, flag: Option<DirectiontFlag>) -> bool {
        match self.waypoint_collection.get_mut(key) {
            Some(point) => {
                point.flag = flag;
                true
            }
            None => false,
        }
    }

    /// Deletes the way point with the indicated key returns as a result if it was possible.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.delete_waypoint(key);
    /// ```
    pub fn delete_waypoint(&mut self, key: WaypointKey) -> Result<(), WaypointKeyNotContained> {
        let deletion = self.waypoint_collection.remove(key);
        if deletion.is_some() {
            Ok(())
        } else {
            Err(WaypointKeyNotContained)
        }
    }
    
}

/// The internal way point image we have as a way point.
#[derive(Debug, Clone)]
pub(crate) enum InternalWaypointImage {
    /// A registered image as a handle.
    Image(Handle),
    /// A cross with an indicated color.
    Cross(Color),
}

/// The way point information stored. This only becomes relevant in deletion, then it
/// can be modified to to be stored as a new waypoint.
#[derive(Debug, Clone)]
pub(crate) struct WaypointInfo {
    /// This contains the graphical representation. Internal, the outside has no
    /// business with the way we hold on to an image or a colour.
    pub(crate) image: InternalWaypointImage,
    /// An optional string that may be drawn in hover over.
    pub(crate) description: Option<String>,
    /// The direction arrow, if the way point is flagged. Set with
    /// [`WaypointSystem::set_flag`].
    pub(crate) flag: Option<DirectiontFlag>,
    /// The mercator rectangle for the waypoint.
    pub(crate) mercator_rectangle: MercatorRectangle
}


impl Annotation for WaypointInfo {
    fn description(&self, _cursor: LatitudeLongitude) -> Option<String> {
        self.description.clone()
    }

    fn cull_bounds(&self) -> &MercatorRectangle {
        &self.mercator_rectangle
    }

    fn hit_test_specific(&self, _position: LatitudeLongitude) -> bool {
        true
    }

    fn label_anchor(&self, _cursor: LatitudeLongitude) -> LabelPosition {
        LabelPosition::FixGeoLocation(self.mercator_rectangle.get_center())
    }

    fn get_flag(&self) -> Option<DirectiontFlag> {
        self.flag
    }

    fn get_render_information(&self) -> RenderingInformation {
        match self.image.clone() {
            InternalWaypointImage::Image(image) => RenderingInformation::Image(image),
            InternalWaypointImage::Cross(color) => RenderingInformation::Cross(color),
        }
    }
}

/// How the direction arrow of a flagged way point looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectiontFlag {
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

        let key = system.add_way_point(
            WaypointSymbol::Cross(Color::WHITE),
            LatitudeLongitude::new(50.0, 7.0),
            None,
        );
        assert_ne!(key, WaypointKey::default());
        let first = system.delete_waypoint(key);
        assert!(first.is_ok(), "We should have some data here.");
        let second = system.delete_waypoint(key);
        assert!(second.is_err(), "That should be empty.");
    }
}
