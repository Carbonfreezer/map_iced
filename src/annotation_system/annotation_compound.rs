//! Contains the compounded subsystems for way points, paths and regions.

use crate::annotation_system::annotation_support::{Annotation, AnnotationError, AnnotationKey};
use crate::annotation_system::path_system::PathSystem;
use crate::annotation_system::region_system::RegionSystem;
use crate::annotation_system::waypoint_system::WaypointSystem;

/// The annotation system as a compound of one separate subsystem per annotation kind:
/// way points, paths and regions. Each kind keeps its own storage, unifying
/// the storage over an enum is not worth the hassle. Only the keys are unified, in
/// [`AnnotationKey`], and the widgets see all kinds through the [`Annotation`] trait.
#[derive(Debug, Clone, Default)]
pub(crate) struct AnnotationSystem {
    pub(crate) waypoint_system: WaypointSystem,
    pub(crate) region_system: RegionSystem,
    pub(crate) path_system: PathSystem,
}

impl AnnotationSystem {
    /// Gets the way point system
    pub fn waypoint_system(&mut self) -> &mut WaypointSystem {
        &mut self.waypoint_system
    }

    /// Asks for the region system.
    pub fn region_system(&mut self) -> &mut RegionSystem {
        &mut self.region_system
    }

    /// Asks for the path system.
    pub fn path_system(&mut self) -> &mut PathSystem {
        &mut self.path_system
    }

    /// All annotations in render order: regions at the bottom, then paths, then way
    /// points on top. The hit test runs through it backwards, so the topmost wins.
    pub(crate) fn get_complete_render_list(
        &self,
    ) -> impl Iterator<Item = (AnnotationKey, &dyn Annotation)> {
        let waypoint = self
            .waypoint_system
            .waypoint_collection
            .iter()
            .map(|(key, value)| (AnnotationKey::Waypoint(key), value as &dyn Annotation));
        let region = self
            .region_system
            .region_collection
            .iter()
            .map(|(key, value)| (AnnotationKey::Region(key), value as &dyn Annotation));
        let path = self
            .path_system
            .path_collection
            .iter()
            .map(|(key, value)| (AnnotationKey::Path(key), value as &dyn Annotation));
        region.chain(path).chain(waypoint)
    }

    /// Sets the description of the annotation `key`, whatever its kind.
    pub(crate) fn update_description(
        &mut self,
        key: AnnotationKey,
        description: Option<String>,
    ) -> Result<(), AnnotationError> {
        // The only way the kinds fail here is a missing key.
        let updated = match key {
            AnnotationKey::Waypoint(key) => self
                .waypoint_system
                .update_waypoint_description(key, description)
                .is_ok(),
            AnnotationKey::Region(key) => self
                .region_system
                .update_description(key, description)
                .is_ok(),
            AnnotationKey::Path(key) => self
                .path_system
                .update_description(key, description)
                .is_ok(),
        };
        updated
            .then_some(())
            .ok_or(AnnotationError::KeyNotContained)
    }

    pub(crate) fn get_specific_element(&self, key: AnnotationKey) -> Result<&dyn Annotation, AnnotationError> {
        match key {
            AnnotationKey::Waypoint(key) => self
                .waypoint_system
                .waypoint_collection
                .get(key)
                .ok_or(AnnotationError::KeyNotContained)
                .map(|value| value as &dyn Annotation),
                // .map(|value| value as &dyn Annotation),
            AnnotationKey::Region(key) => self
                .region_system
                .region_collection
                .get(key)
                .ok_or(AnnotationError::KeyNotContained)
                .map(|value| value as &dyn Annotation),
            AnnotationKey::Path(key) => self
                .path_system
                .path_collection
                .get(key)
                .ok_or(AnnotationError::KeyNotContained)
                .map(|value| value as &dyn Annotation),
        }
    }
}
