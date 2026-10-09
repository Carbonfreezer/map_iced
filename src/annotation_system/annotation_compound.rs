//! Contains the compounded subsystems for  waypoints and later pathes and regions.

use crate::annotation_system::annotation_support::{Annotation, AnnotationKey};
use crate::annotation_system::waypoint_system::WaypointSystem;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub(crate) struct AnnotationSystem {
    pub(crate) waypoint_system: WaypointSystem,
}

impl AnnotationSystem {
    /// Gets the way point system
    pub fn waypoint_system(&mut self) -> &mut WaypointSystem {
        &mut self.waypoint_system
    }

    pub(crate) fn get_complete_render_list(
        &self,
    ) -> impl Iterator<Item = (AnnotationKey, &dyn Annotation)> {
        // TODO: Has to be chained with the other elements,
        self.waypoint_system
            .waypoint_collection
            .iter()
            .map(|(key, value)| (AnnotationKey::Waypoint(key), value as &dyn Annotation))
    }

    pub(crate) fn get_specific_element(&self, key: AnnotationKey) -> Option<Box<dyn Annotation>> {
        match key {
            AnnotationKey::Waypoint(key) => {
                let value = self.waypoint_system.waypoint_collection.get(key)?;
                Some(Box::new(value.clone()))
            }
        }
    }
}
