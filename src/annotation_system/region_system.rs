//! This system represents regions that can be selected and annotated and zoomed to.

use geo::{point, Contains, Polygon};
use slotmap::{new_key_type, SlotMap};
use crate::annotation_system::annotation_support::{Annotation, LabelPosition, MercatorRectangle, RenderingClosedShape, RenderingInformation};
use crate::annotation_system::waypoint_system::DirectionFlag;
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
    // TODO: The next big block to be implemented.
}
