//! This module contains as a pseudo double linked list the entries of the data in least recently
//! used priority que. The data stored is an u64. This is the core data used for the tiling system.

use crate::tile_cache::file_util::{FileUtil, TileCollection};
use fxhash::{FxHashMap, FxHashSet};
use std::num::NonZeroU32;

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
    /// The amount of data we store as the accumulated total file size.
    amount_of_data: u64,
}

impl LastRecentlyUsedList {
    /// Takes the entry and moves it to the front. Returns if we need to save the structure.
    fn touch(&mut self, index: u32) {
        // When we are the first in the list there is nothing to touch.
        let Some(previous) = get_u32(self.entry_list[index as usize].previous) else {
            debug_assert_eq!(
                Some(index),
                self.first_entry,
                "Counter check that we are the first entry."
            );
            return;
        };

        // The old first element has now us as the predecessor.
        if let Some(first) = self.first_entry {
            self.entry_list[first as usize].previous = set_u32(index);
        }

        // The old previous now has our next as the follow-up.
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
    /// Either way the entry will always be on the top.
    pub fn touch_or_insert(&mut self, data: u64) {
        if let Some(index) = self.forward_map.get(&data) {
            self.touch(*index)
        } else {
            self.generate_new_entry(data);
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

    /// Generates an entry for the LRU table.
    fn get_lru_entry(index: u32, last_element: u32, content: u64) -> LRUEntry {
        LRUEntry {
            previous: (index > 0).then(|| set_u32(index - 1).unwrap()),
            next: (index < last_element).then(|| set_u32(index + 1).unwrap()),
            data: content,
        }
    }

    /// Generates an LRU eviction system from an u64 slice handed over. This method
    /// is meant as an entry point for loading. We want to get all elements of the
    /// lru content, that has also a file handle in lodaded handles and then add the handles in
    /// loaded handles, that are not in lru_content. So we have exactly the loaded_file_handles in there
    /// in the end just in different order.
    pub fn reconstruct_from(&mut self, lru_content: &[u64], existing_entries: &TileCollection) {
        let total_size = existing_entries.tile_ids.len();
        if total_size == 0 {
            *self = Self::default();
            return;
        }
        self.amount_of_data = existing_entries.total_file_size;
        debug_assert!(self.amount_of_data > 0, "As we have files we should have a larger file size.");

        let last_element = total_size as u32 - 1;
        self.open_spaces.clear();
        self.forward_map.clear();
        self.forward_map.reserve(total_size);
        let mut sequence_content: Vec<LRUEntry> = Vec::with_capacity(total_size);
        let mut remaining_loaded_files = FxHashSet::from_iter(&existing_entries.tile_ids);
        debug_assert_eq!(
            remaining_loaded_files.len(),
            total_size,
            "The loaded files should be unique per construction"
        );

        let mut position_counter = 0;
        // First pass we go over the lru content.
        for &x in lru_content {
            // Skip all entries that are not loaded.
            if !remaining_loaded_files.remove(&x) {
                continue;
            }
            sequence_content.push(Self::get_lru_entry(position_counter, last_element, x));
            self.forward_map.insert(x, position_counter);
            position_counter += 1;
        }
        // Now we add all the remaining files, that are loaded but not in the content table.
        for &x in remaining_loaded_files {
            sequence_content.push(Self::get_lru_entry(position_counter, last_element, x));
            self.forward_map.insert(x, position_counter);
            position_counter += 1;
        }

        debug_assert_eq!(sequence_content.len(), total_size);
        debug_assert_eq!(position_counter, total_size as u32);

        self.entry_list = sequence_content;
        self.first_entry = Some(0);
        self.last_entry = Some(last_element);
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
    fn filter_test() {
        let raw_data = vec![0, 1, 2, 3, 4, 5];
        let file_data = TileCollection{ tile_ids: vec![5, 2, 3, 22], total_file_size: 20};
        let expected_data = vec![2, 3, 5, 22];
        let mut cand = LastRecentlyUsedList::default();
        cand.reconstruct_from(&raw_data, &file_data);
        assert_eq!(cand.generate_usage_list(), expected_data);
    }

    #[test]
    fn fill_test() {
        let mut cand = LastRecentlyUsedList::default();
        let total_vec = vec![0, 1, 2, 3, 4];
        for i in total_vec.iter().rev() {
            cand.touch_or_insert(*i);
        }

        let usage_list = cand.generate_usage_list();

        assert_eq!(cand.generate_usage_list(), vec![0, 1, 2, 3, 4]);
        cand.touch_or_insert(0);
        assert_eq!(cand.generate_usage_list(), vec![0, 1, 2, 3, 4]);
        cand.touch_or_insert(4);
        assert_eq!(cand.generate_usage_list(), vec![4, 0, 1, 2, 3]);
        cand.reconstruct_from(&usage_list, &TileCollection{ tile_ids: total_vec, total_file_size: 20});
        cand.touch_or_insert(2);
        assert_eq!(cand.generate_usage_list(), vec![2, 0, 1, 3, 4]);
    }

    #[test]
    fn reconstruct_test() {
        let mut cand = LastRecentlyUsedList::default();

        cand.reconstruct_from(&[], &TileCollection{tile_ids: vec![0, 1, 2, 3, 4, 5], total_file_size: 20});
        let mut list = cand.generate_usage_list();
        list.sort();
        assert_eq!(list, vec![0, 1, 2, 3, 4, 5]);
    }
}
