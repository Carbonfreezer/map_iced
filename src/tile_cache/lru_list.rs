//! This module contains as a pseudo double linked list the entries of the data in least recently
//! used priority que. The data stored is an u64. This is the core data used for the tiling system.

use crate::tile_cache::file_util::{FileUtil, TileCollection};
use fxhash::FxHashMap;
use std::num::NonZeroU32;

/// After how many hit attempts does the file need resaving.
const SAVE_REQUIRED_AFTER: u32 = 20;

// Here are some helper functions to deal with Option<NonZeroU32>

fn get_u32(x: Option<NonZeroU32>) -> Option<u32> {
    x.map(|x| x.get() - 1)
}

fn set_u32(x: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(x + 1)
}

fn map_u32(x: Option<u32>) -> Option<NonZeroU32> {
    x.map(|x| NonZeroU32::new(x + 1).unwrap())
}

#[derive(Debug, Clone, Default)]
struct LRUEntry {
    previous: Option<NonZeroU32>,
    next: Option<NonZeroU32>,
    data: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LastRecentlyUsedList {
    /// The entry lost with the least recently used stuff.
    entry_list: Vec<LRUEntry>,
    /// The stak with the free entries.
    open_spaces: Vec<u32>,
    /// The position of the first entry.
    first_entry: Option<u32>,
    /// The last entry in the list.
    last_entry: Option<u32>,
    /// The hashmap to find which data is stored in which cell.
    forward_map: FxHashMap<u64, u32>,
    /// Counts the amount of hit attempts needed to check for saving.
    hit_attempts: u32,
}

impl LastRecentlyUsedList {
    /// Takes the entry and moves it to the front. Returns if we need to save the structure.
    fn touch(&mut self, index: u32) -> bool {
        // When we are the first in the list there is nothing to touch.
        let Some(previous) = get_u32(self.entry_list[index as usize].previous) else {
            debug_assert_eq!(
                Some(index),
                self.first_entry,
                "Counter check that we are the first entry."
            );
            return false;
        };

        // The old first element has now us as the predecessor.
        if let Some(first) = self.first_entry {
            self.entry_list[first as usize].previous = set_u32(index);
        }

        // The old previous now has our next as the follow up.
        self.entry_list[previous as usize].next = self.entry_list[index as usize].next;

        // Check if we were the last entry on that list.
        if let Some(next) = get_u32(self.entry_list[index as usize].next) {
            self.entry_list[next as usize].previous = self.entry_list[index as usize].previous;
        } else {
            // In this case we were at the end of the list and the new last entry becomes our previous.
            debug_assert_eq!(
                Some(index),
                self.last_entry,
                "Counter check that we are the last entry."
            );
            self.last_entry = Some(previous);
        }

        // Now we have to adjust our pointers.
        self.entry_list[index as usize].previous = None;
        self.entry_list[index as usize].next = map_u32(self.first_entry);
        self.first_entry = Some(index);

        self.hit_attempts += 1;
        if self.hit_attempts >= SAVE_REQUIRED_AFTER {
            self.hit_attempts = 0;
            true
        } else {
            false
        }
    }

    /// Generates a new entry at the top and returns the storage entry.
    fn generate_new_entry(&mut self, data: u64) {
        // First we have to eventually add a new entry.
        let space = match self.open_spaces.pop() {
            Some(n) => n,
            None => {
                self.entry_list.push(LRUEntry::default());
                self.entry_list.len() as u32 - 1
            }
        };
        let element = &mut self.entry_list[space as usize];
        element.data = data;
        element.previous = None;
        element.next = map_u32(self.first_entry);
        if let Some(entry) = self.first_entry {
            self.entry_list[entry as usize].previous = set_u32(space);
        }
        self.first_entry = Some(space);
        if self.last_entry.is_none() {
            self.last_entry = self.first_entry;
        }

        self.forward_map.insert(data, space);
    }

    /// Touches the data if existing or generates a new entry,
    /// Either way the entry will always be on the top. We also returns whether
    /// we require a save.
    pub fn touch_or_insert(&mut self, data: u64) -> bool {
        if let Some(index) = self.forward_map.get(&data) {
            self.touch(*index)
        } else {
            self.generate_new_entry(data);
            self.hit_attempts = 0;
            true
        }
    }

    /// Frees elements from the Cache and returns the freed elements.
    /// If the element does not exist anymore a size of zero should be returned.
    /// It will get cued up in the deletion list anyway and should be ignored from the caller.
    /// Returns the files to eliminate including their total memory consumption.
    pub async fn free_elements(
        &mut self,
        mut amount_to_free: u64,
        file_util: &FileUtil,
    ) -> TileCollection {
        let mut free_list = Vec::with_capacity(self.forward_map.len());
        let mut freed_accumulated = 0;
        while let Some(scan) = self.last_entry
            && amount_to_free != 0
        {
            let element = &self.entry_list[scan as usize];
            let content = element.data;
            self.forward_map.remove(&content);
            self.open_spaces.push(scan);
            self.last_entry = get_u32(element.previous);
            if let Some(previous) = self.last_entry {
                self.entry_list[previous as usize].next = None;
            }

            let freed_amount = file_util.get_file_length_from_id(content).await;
            // Only enlist the file for removal, if memory consumption is larger 0 otherwise it does not exist.
            if freed_amount > 0 {
                freed_accumulated += freed_amount;
                amount_to_free = amount_to_free.saturating_sub(freed_amount);
                free_list.push(content);
            }
        }

        // Check if we have flushed all.
        if self.last_entry.is_none() {
            self.first_entry = None;
        }

        // Reset the counter, we have to do a save here afterward anyway.
        self.hit_attempts = 0;

        TileCollection {
            tile_ids: free_list,
            total_file_size: freed_accumulated,
        }
    }

    /// Generates a list from the current cache list from most to least recently used.
    /// This method is meant as a preparation for saving.
    pub fn generate_usage_list(&self) -> Vec<u64> {
        let mut result = Vec::with_capacity(self.forward_map.len());
        let mut scan = self.first_entry;
        while let Some(entry) = scan {
            result.push(self.entry_list[entry as usize].data);
            scan = get_u32(self.entry_list[entry as usize].next);
        }

        result
    }

    /// Gets called after loading and makes sure, that all elements in the list are registered.
    pub fn complete_list(&mut self, candidates: &[u64]) {
        let forgotten_entries: Vec<_> = candidates
            .iter()
            .filter(|&x| !self.forward_map.contains_key(x))
            .collect();
        for x in forgotten_entries {
            self.generate_new_entry(*x);
        }
    }

    /// Generates an LRU eviction system from an u64 slice handed over. This method
    /// is meant as an entry point for loading.
    pub fn reconstruct_from(&mut self, content_slice: &[u64])  {
        if content_slice.is_empty() {
            return;
        }

        let last_element = content_slice.len() - 1;
        self.forward_map.clear();
        self.forward_map.reserve(last_element + 1);

        let content = content_slice
            .iter()
            .enumerate()
            .inspect(|(i, x)| {
                self.forward_map.insert(**x, *i as u32);
            })
            .map(|(i, x)| LRUEntry {
                previous: (i > 0).then(|| set_u32(i as u32 - 1).unwrap()),
                next: (i < last_element).then(|| set_u32(i as u32 + 1).unwrap()),
                data: *x,
            })
            .collect();

        self.entry_list = content;
        self.first_entry = Some(0);
        self.last_entry = Some(last_element as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn empty_test() {
        let cand = LastRecentlyUsedList::default();
        assert_eq!(cand.generate_usage_list(), Vec::new());
    }

    #[test]
    fn fill_test() {
        let mut cand = LastRecentlyUsedList::default();
        for i in (0..5).rev() {
            cand.touch_or_insert(i);
        }

        let usage_list = cand.generate_usage_list();
        assert_eq!(cand.generate_usage_list(), vec![0, 1, 2, 3, 4]);
        cand.touch_or_insert(0);
        assert_eq!(cand.generate_usage_list(), vec![0, 1, 2, 3, 4]);
        cand.touch_or_insert(4);
        assert_eq!(cand.generate_usage_list(), vec![4, 0, 1, 2, 3]);
        cand.reconstruct_from(&usage_list);
        cand.touch_or_insert(2);
        assert_eq!(cand.generate_usage_list(), vec![2, 0, 1, 3, 4]);
    }



    #[test]
    fn reconstruct_test() {
        let mut cand = LastRecentlyUsedList::default();
        cand.reconstruct_from(&[0, 1, 2, 3, 4,5 ]);
        let list = cand.generate_usage_list();
        assert_eq!(list, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn completion_test() {
        let mut cand = LastRecentlyUsedList::default();
        cand.reconstruct_from(&[0, 1, 2, 3]);
        cand.complete_list(&[2, 3, 4, 5]);
        assert_eq!(cand.generate_usage_list(), vec![5, 4, 0, 1, 2, 3]);
    }
    
}
