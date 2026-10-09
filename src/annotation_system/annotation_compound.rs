//! Contains the compounded subsystems for  waypoints and later pathes and regions.

use crate::annotation_system::annotation_support::{Annotation, AnnotationKey};
use crate::annotation_system::waypoint_system::WaypointSystem;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct AnnotationCompound {
    waypoint_system: WaypointSystem,
}

impl AnnotationCompound {
    /// Gets the way point system
    pub fn waypoint_system(&mut self) -> &mut WaypointSystem {
        &mut self.waypoint_system
    }

    /// Gets the complete content in drawing order of regions, pathes waypoints later on.
    pub fn get_complete_render_list(&self) -> impl Iterator<Item=AnnotationKey> {
        self.waypoint_system.waypoint_collection.keys().map(|key| AnnotationKey::Waypoint(key))
    }

    /// Asks for a specific element as a reference.
    pub(crate) fn get_element(&self, key: AnnotationKey) -> Option<&dyn Annotation> {
        match key {
            AnnotationKey::Waypoint(key) => self
                .waypoint_system
                .waypoint_collection
                .get(key)
                .map(|x| x as &dyn Annotation),
        }
    }
}
