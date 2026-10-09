//! Contains the compounded subsystems for  waypoints and later pathes and regions.

use std::sync::Arc;
use crate::annotation_system::annotation_support::{Annotation, AnnotationKey};
use crate::annotation_system::waypoint_system::WaypointSystem;

#[derive(Debug, Clone, Default)]
pub struct AnnotationCompound {
    waypoint_system: WaypointSystem,
}

pub type AnnotationType = (AnnotationKey, Arc<dyn Annotation>);

impl AnnotationCompound {
    /// Gets the way point system
    pub fn waypoint_system(&mut self) -> &mut WaypointSystem {&mut self.waypoint_system}

    /// Gets the complete content in drawing order of regions, pathes waypoints later on.
    pub fn get_complete_render_list(&self) -> Vec<AnnotationType> {
        let mut result : Vec<AnnotationType> = Vec::new();
        for (key, value) in self.waypoint_system.waypoint_collection.iter() {
            result.push((AnnotationKey::Waypoint( key.clone()), Arc::new(value.clone())));
        }

        result
    }
}
