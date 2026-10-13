//! This system represents regions that can be selected and annotated and zoomed to.

use geo::{point, Contains, Polygon};
use iced::Color;
use slotmap::{new_key_type, SlotMap};
use crate::annotation_system::annotation_support::{Annotation, LabelPosition, MercatorRectangle, RenderingClosedShape, RenderingInformation};
use crate::annotation_system::waypoint_system::{DirectionFlag};
use crate::gui_system::latitude_longitude::LatitudeLongitude;

new_key_type! {
    /// The handle of a way point inside a [`RegionSystem`]. A type of its own, so
    /// that it cannot be confused with the handles of the other annotation kinds.
    pub struct RegionKey;
}



/// A that may represent different shapes.
#[derive(Debug, Clone)]
pub struct Region {
    /// The mercator rectangle of the region.
    bounds : MercatorRectangle,
    /// The polygon in geo coordinates.
    geo_polygon : Polygon,
    /// The rendering information.
    rendering_info: RenderingClosedShape,
    /// The annotation text of the region.
    /// An optional string that may be drawn in hover over.
    pub(crate) description: Option<String>,
    /// The direction arrow, if the region is flagged. Set with
    /// [`RegionSystem::set_flag`].
    pub(crate) flag: Option<DirectionFlag>,
}

impl Annotation for Region {
    fn description(&self, _cursor: LatitudeLongitude) -> Option<String> {
        self.description.clone()
    }

    fn cull_bounds(&self) -> &MercatorRectangle {
        &self.bounds
    }

    fn get_center(&self) -> LatitudeLongitude {
        self.bounds.get_center()
    }

    fn hit_test_specific(&self, position: LatitudeLongitude) -> bool {
        self.geo_polygon.contains(&point!(x: position.longitude, y: position.latitude))
    }

    fn label_anchor(&self, _cursor: LatitudeLongitude) -> LabelPosition {
        LabelPosition::AtMousePosition
    }

    fn get_flag(&self) -> Option<DirectionFlag> {
        self.flag
    }

    fn get_render_information(&self) -> RenderingInformation<'_> {
        RenderingInformation::Polygon(&self.rendering_info)
    }

    fn clone_box(&self) -> Box<dyn Annotation> {
        Box::new(self.clone())
    }
}

/// The region system for all the different regions.
#[derive(Debug, Clone, Default)]
pub struct RegionSystem {
    pub(crate) region_collection: SlotMap<RegionKey, Region>,
}

/// Error type for that case that the region is not contained in the structure.
#[derive(Debug, Clone, Copy)]
pub struct RegionKeyNotContained;

impl RegionSystem {
    // TODO: Examples need to get added.-

    /// Adds a circular region to the system and returns the key.
    pub fn add_circular_region(
        &mut self,
        position: LatitudeLongitude,
        radius: f64,
        inner_color : Color,
        border_color : Color,
        description: Option<String>,
    ) -> RegionKey {
        let rendering_info = RenderingClosedShape {
            positions_ccw : position.create_circle_around(radius, 128),
            core_color: inner_color,
            border_color,
        };

        let geo_polygon = Polygon::new(rendering_info.positions_ccw.iter().map(|pos|(pos.longitude, pos.latitude)).collect() ,vec![]);
        let bounds = MercatorRectangle::create_from_position_array(rendering_info.positions_ccw.iter().map(|x| *x), 0.0);

        let region = Region {
            bounds,
            geo_polygon,
            rendering_info,
            description,
            flag: None
        };
        self.region_collection.insert(region)
    }

    /// Changes the coloring of the region.
    pub fn update_color(&mut self, key: RegionKey, inner_color : Color, border_color : Color) -> Result<(), RegionKeyNotContained> {
        let render_info = &mut self.region_collection.get_mut(key).ok_or(RegionKeyNotContained)?.rendering_info;
        render_info.core_color = inner_color;
        render_info.border_color = border_color;
        Ok(())
    }

    /// Sets a new annotation description.
    pub fn update_description(&mut self, key: RegionKey, description : Option<String>) -> Result<(), RegionKeyNotContained> {
        self.region_collection
            .get_mut(key)
            .ok_or(RegionKeyNotContained)?
            .description = description;
        Ok(())
    }

    /// Sets the arrow flag for the region.
    pub fn set_flag(&mut self, key: RegionKey, flag: Option<DirectionFlag>) -> Result<(), RegionKeyNotContained> {
        self.region_collection.get_mut(key).ok_or(RegionKeyNotContained)?.flag = flag;
        Ok(())
    }

    /// Removes the region key from the list.
    pub fn delete_region(&mut self, key: RegionKey) -> Result<(), RegionKeyNotContained> {
        let deletion = self.region_collection.remove(key);
        if deletion.is_some() {
            Ok(())
        } else {
            Err(RegionKeyNotContained)
        }
    }


}
