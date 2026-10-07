//! This module consists of an encapsulation of FxHashmap, that can directly give an iterator to
//! iterate over the elements in the sequence they are handed over.

use std::hash::Hash;
use fxhash::FxHashMap;


/// The stable hashmap.
pub(crate) struct StableHashmap<Key, Value> where Key: Hash + Eq  {
    internal_map : FxHashMap<Key, usize>,
    internal_vector : Vec<Value>
}

impl<Key, Value> StableHashmap<Key, Value>
    where Key: Hash + Eq
{
    /// Creates the hashmap from a key value pair iterator stream.-
    pub(crate) fn new(input : impl Iterator<Item = (Key, Value)>) -> Self {
        let mut internal_map = FxHashMap::default();
        let mut internal_vector = Vec::new();
        for (key, val) in input {
            internal_vector.push(val);
            debug_assert!(internal_map.get(&key).is_none(), "The key is already used.");
            internal_map.insert(key, internal_vector.len() - 1);
        }

        Self {
            internal_map,
            internal_vector
        }
    }

    /// Tries to get the element with the specific key.
    /// Returns the value if possible.
    pub(crate) fn get(&self, key: &Key) -> Option<&Value> {
        self.internal_vector.get(*self.internal_map.get(key)?)
    }

    /// Asks for the internal iterator that returns the elements in the sequence as handed over in
    /// construction.
    pub(crate) fn get_iterator(&self) -> impl Iterator<Item = &Value> + '_ {
        self.internal_vector.iter()
    }
}

#[cfg(test)]
mod tests {
    use slotmap::SlotMap;
    use super::*;
    #[test]
    fn stable_hash_test() {
        let mut base = SlotMap::new();
        let input = ["foo".to_string(), "bar".to_string(), "baz".to_string()];
        let keys : Vec<_>= input.iter().map(|x| base.insert(x.clone())).collect();
        let stable = StableHashmap::new(base.into_iter());
        for (key, value) in keys.iter().zip(input.iter()) {
            assert_eq!(stable.get(key), Some(value));
        }

        for (x,y) in stable.get_iterator().zip(input.iter()) {
            assert_eq!(x, y);
        }
    }
}