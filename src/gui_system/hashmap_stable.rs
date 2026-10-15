//! This module consists of an encapsulation of FxHashmap, that can directly give an iterator to
//! iterate over the elements in the sequence they are handed over.

use crate::annotation_system::annotation_support::{Annotation, AnnotationKey};
use fxhash::FxHashMap;

/// The stable hashmap.
#[derive(Default, Debug)]
pub(crate) struct HashmapStable {
    internal_map: FxHashMap<AnnotationKey, usize>,
    internal_vector: Vec<(AnnotationKey, Box<dyn Annotation>)>,
}

impl HashmapStable {
    /// Creates the hashmap from a key value pair iterator stream.-
    pub(crate) fn new(input: impl Iterator<Item = (AnnotationKey, Box<dyn Annotation>)>) -> Self {
        let mut internal_map = FxHashMap::default();
        let mut internal_vector = Vec::new();
        for (key, val) in input {
            internal_vector.push((key, val));
            debug_assert!(!internal_map.contains_key(&key), "The key is already used.");
            internal_map.insert(key, internal_vector.len() - 1);
        }

        Self {
            internal_map,
            internal_vector,
        }
    }

    /// Tries to get the element with the specific key.
    /// Returns the value if possible.
    pub(crate) fn get(&self, key: &AnnotationKey) -> Option<&dyn Annotation> {
        self.internal_vector
            .get(*self.internal_map.get(key)?)
            .map(|(_, val)| val.as_ref())
    }

    /// Replaces the element with the specific key, keeping its place in the sequence.
    /// Returns `false` and changes nothing if the key is not contained.
    pub(crate) fn replace(&mut self, key: &AnnotationKey, value: Box<dyn Annotation>) -> bool {
        let Some(&index) = self.internal_map.get(key) else {
            return false;
        };
        self.internal_vector[index].1 = value;
        true
    }

    /// Asks for the internal iterator that returns the elements in the sequence as handed over in
    /// construction. Gives elements as key value tuples.
    pub(crate) fn get_iterator(
        &self,
    ) -> impl DoubleEndedIterator<Item = (AnnotationKey, &dyn Annotation)> {
        self.internal_vector
            .iter()
            .map(|(key, val)| (*key, val.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_system::waypoint_system::{WaypointSymbol, WaypointSystem};
    use crate::gui_system::latitude_longitude::LatitudeLongitude;
    use iced::Color;

    #[test]
    fn stable_hash_test() {
        let mut system = WaypointSystem::default();
        let names = ["foo", "bar", "baz"];
        let keys: Vec<AnnotationKey> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                system
                    .add_way_point(
                        WaypointSymbol::cross(Color::WHITE),
                        LatitudeLongitude::new(50.0, 7.0 + index as f64),
                        Some(name.to_string()),
                    )
                    .into()
            })
            .collect();
        let cursor = LatitudeLongitude::new(0.0, 0.0);
        let stable = HashmapStable::new(
            system
                .waypoint_collection
                .iter()
                .map(|(key, value)| (AnnotationKey::Waypoint(key), value.clone_box())),
        );

        for (key, name) in keys.iter().zip(names) {
            let annotation = stable.get(key).expect("Key should be present");
            assert_eq!(annotation.description(cursor).as_deref(), Some(name));
        }

        let iterated: Vec<_> = stable.get_iterator().collect();
        assert_eq!(iterated.len(), names.len());
        for ((key, annotation), name) in iterated.iter().zip(names) {
            assert_eq!(annotation.description(cursor).as_deref(), Some(name));
            assert_eq!(
                stable.get(key).unwrap().description(cursor).as_deref(),
                Some(name)
            );
        }

        // A key that did not make it into the snapshot is not found.
        let missing = system.add_way_point(
            WaypointSymbol::cross(Color::WHITE),
            LatitudeLongitude::new(0.0, 0.0),
            None,
        );
        assert!(stable.get(&missing.into()).is_none());

        // Replacing keeps the place in the sequence, a missing key changes nothing.
        let mut stable = stable;
        let mut renamed = system.waypoint_collection[missing].clone();
        renamed.description = Some("renamed".to_string());
        let AnnotationKey::Waypoint(middle) = keys[1] else {
            unreachable!("way point keys only");
        };
        let mut middle_renamed = system.waypoint_collection[middle].clone();
        middle_renamed.description = Some("qux".to_string());
        assert!(stable.replace(&keys[1], Box::new(middle_renamed)));
        assert!(!stable.replace(&missing.into(), Box::new(renamed)));
        let descriptions: Vec<_> = stable
            .get_iterator()
            .map(|(_, annotation)| annotation.description(cursor))
            .collect();
        assert_eq!(
            descriptions,
            ["foo", "qux", "baz"].map(|name| Some(name.to_string()))
        );
    }
}
