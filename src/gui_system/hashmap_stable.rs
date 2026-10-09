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
