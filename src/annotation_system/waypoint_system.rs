//! This module administrates the way points, A way point is a single isolated location on the map.
//! This differentiates from the paths, where several points are interconnected with each other.

use crate::annotation_system::annotation_support::{
    Annotation, ClosestPoint, DirectionFlag, LabelPosition, MercatorRectangle, RenderingInformation,
};
use crate::gui_system::internal_math::MercatorPoint;
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::map_widget_components::FocusTarget;
/// Re-exported so that callers who already hold a `Bytes` for [`WaypointSymbol::image`]
/// do not have to depend on `bytes` themselves and keep its version in step with ours.
/// Everyone else passes a `Vec<u8>` or `&'static [u8]`.
pub use bytes::Bytes;
use iced::Color;
use iced::advanced::image::Handle;
use slotmap::{SlotMap, new_key_type};

/// Half the size of a way point symbol in pixels. It is at the same time the pixel
/// padding of the way point's [`MercatorRectangle`], which the widget uses for
/// culling, hit testing and the size of the drawn symbol. Widening the padding
/// therefore also grows the symbol.
const WAYPOINT_HALF_SIZE: f64 = 12.0;

new_key_type! {
    /// The handle of a way point inside a [`WaypointSystem`]. A type of its own, so
    /// that it cannot be confused with the handles of the other annotation kinds.
    ///
    /// It is `Copy`, `Eq`, `Hash` and `Ord`, so data of your own about a way point
    /// can live in a `HashMap` or `BTreeMap` keyed by it.
    pub struct WaypointKey;
}

/// The way point parameter symbol we paint over the map, built with
/// [`WaypointSymbol::image`] or [`WaypointSymbol::cross`].
#[derive(Debug, Clone)]
pub struct WaypointSymbol(InternalWaypointImage);

impl WaypointSymbol {
    /// An image symbol from the encoded image file data, e.g. a `Vec<u8>` read at
    /// runtime or a `&'static [u8]` from `include_bytes!`, without naming [`Bytes`].
    ///
    /// # Example
    /// ```
    /// use map_iced::annotation_system::waypoint_system::WaypointSymbol;
    /// const ICON: &[u8] = include_bytes!("../../assets/Test.png");
    /// let symbol = WaypointSymbol::image(ICON);
    /// ```
    pub fn image(data: impl Into<Bytes>) -> Self {
        WaypointSymbol(InternalWaypointImage::Image(Handle::from_bytes(
            data.into(),
        )))
    }

    /// A cross drawn in `color`.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::WaypointSymbol;
    /// let symbol = WaypointSymbol::cross(Color::from_rgb(1.0, 0.0, 0.0));
    /// ```
    pub fn cross(color: Color) -> Self {
        WaypointSymbol(InternalWaypointImage::Cross(color))
    }
}

/// The administration for the whole waypoint system.
#[derive(Debug, Clone, Default)]
pub struct WaypointSystem {
    /// The collection as a slot map.
    pub(crate) waypoint_collection: SlotMap<WaypointKey, WaypointInfo>,
}

/// The errors of the [`WaypointSystem`]. A method that fails changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaypointError {
    /// There is no way point with this key, or not any more.
    KeyNotContained,
}

impl WaypointSystem {
    /// Adds a new waypoint to the system.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// ```
    pub fn add_way_point(
        &mut self,
        symbol: WaypointSymbol,
        position: LatitudeLongitude,
        description: Option<String>,
    ) -> WaypointKey {
        self.waypoint_collection.insert(WaypointInfo {
            image: symbol.0,
            mercator_rectangle: MercatorRectangle::create_from_position(
                position,
                WAYPOINT_HALF_SIZE,
            ),
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
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.update_waypoint_position(key, LatitudeLongitude::new(50.0, 8.0));
    /// ```
    pub fn update_waypoint_position(
        &mut self,
        key: WaypointKey,
        position: LatitudeLongitude,
    ) -> Result<(), WaypointError> {
        self.waypoint_collection
            .get_mut(key)
            .ok_or(WaypointError::KeyNotContained)?
            .mercator_rectangle =
            MercatorRectangle::create_from_position(position, WAYPOINT_HALF_SIZE);
        Ok(())
    }

    /// Sets the image / symbol of an existing way point.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.update_waypoint_symbol(key, WaypointSymbol::cross(Color::BLACK));
    /// ```
    pub fn update_waypoint_symbol(
        &mut self,
        key: WaypointKey,
        symbol: WaypointSymbol,
    ) -> Result<(), WaypointError> {
        self.waypoint_collection
            .get_mut(key)
            .ok_or(WaypointError::KeyNotContained)?
            .image = symbol.0;
        Ok(())
    }

    /// Sets the (optional) description of an existing way point.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.update_waypoint_description(key, Some("Annotation".to_string()));
    /// ```
    pub fn update_waypoint_description(
        &mut self,
        key: WaypointKey,
        description: Option<String>,
    ) -> Result<(), WaypointError> {
        self.waypoint_collection
            .get_mut(key)
            .ok_or(WaypointError::KeyNotContained)?
            .description = description;
        Ok(())
    }

    /// Flags the way point, or takes the flag away with `None`. A flagged way point
    /// gets an arrow on the edge of every widget it is off screen in.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::annotation_support::DirectionFlag;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let flag = DirectionFlag { color: Color::from_rgb(1.0, 0.0, 0.0), priority: 3 };
    /// assert!(system.set_flag(key, Some(flag)).is_ok());
    /// ```
    pub fn set_flag(
        &mut self,
        key: WaypointKey,
        flag: Option<DirectionFlag>,
    ) -> Result<(), WaypointError> {
        self.waypoint_collection
            .get_mut(key)
            .ok_or(WaypointError::KeyNotContained)?
            .flag = flag;
        Ok(())
    }

    /// Deletes the way point with the indicated key.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = WaypointSystem::default();
    /// let key = system.add_way_point(WaypointSymbol::cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),None);
    /// let _ = system.delete_waypoint(key);
    /// ```
    pub fn delete_waypoint(&mut self, key: WaypointKey) -> Result<(), WaypointError> {
        let deletion = self.waypoint_collection.remove(key);
        if deletion.is_some() {
            Ok(())
        } else {
            Err(WaypointError::KeyNotContained)
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

/// A way point as stored in the [`WaypointSystem`].
#[derive(Debug, Clone)]
pub(crate) struct WaypointInfo {
    /// This contains the graphical representation. Internal, the outside has no
    /// business with the way we hold on to an image or a colour.
    pub(crate) image: InternalWaypointImage,
    /// An optional string that may be drawn in hover over.
    pub(crate) description: Option<String>,
    /// The direction arrow, if the way point is flagged. Set with
    /// [`WaypointSystem::set_flag`].
    pub(crate) flag: Option<DirectionFlag>,
    /// The mercator rectangle for the waypoint.
    pub(crate) mercator_rectangle: MercatorRectangle,
}

impl Annotation for WaypointInfo {
    fn description(&self, _cursor: LatitudeLongitude) -> Option<String> {
        self.description.clone()
    }

    fn cull_bounds(&self) -> &MercatorRectangle {
        &self.mercator_rectangle
    }

    /// A way point has no extent, its padded bounds are the hit area.
    fn closest_point(&self, _position: MercatorPoint) -> ClosestPoint {
        ClosestPoint::Inside
    }

    /// A way point has no extent to fit, the focus keeps the zoom level.
    fn focus_target(&self, _fill: f32) -> FocusTarget {
        FocusTarget::Position(self.get_center())
    }

    fn label_anchor(&self, _cursor: LatitudeLongitude) -> LabelPosition {
        LabelPosition::FixGeoLocation(self.mercator_rectangle.get_center())
    }

    fn get_flag(&self) -> Option<DirectionFlag> {
        self.flag
    }

    fn get_render_information(&self) -> RenderingInformation<'_> {
        match self.image.clone() {
            InternalWaypointImage::Image(image) => RenderingInformation::Image(image),
            InternalWaypointImage::Cross(color) => RenderingInformation::Cross(color),
        }
    }

    fn clone_box(&self) -> Box<dyn Annotation> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_deletion() {
        let mut system = WaypointSystem::default();

        let key = system.add_way_point(
            WaypointSymbol::cross(Color::WHITE),
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
