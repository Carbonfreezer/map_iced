//! This module contains as a pseudo double linked list the entries of the data in least recently
//! used priority que. The data stored is an u64. This is the core data used for the tiling system.

use crate::tile_cache::file_util::{TileData};
use fxhash::FxHashMap;
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
    /// The data we represent.
    data: u64,
    /// The amount of disc space we occupy.
    disc_space_consumption : u64
}

#[derive(Debug, Clone)]
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
    space_on_disc: u64,
    /// The maximum amount of data we may have on the disc.
    max_space_on_disc: u64,
}

impl LastRecentlyUsedList {

    /// Creates a new least recently used list.
    pub fn new(max_space_on_disc: u64) -> Self {
        Self {
            entry_list: Default::default(),
            open_spaces: Default::default(),
            first_entry: Default::default(),
            last_entry: Default::default(),
            space_on_disc: Default::default(),
            forward_map: Default::default(),
            max_space_on_disc
        }
    }

    /// Resets the data we have.
    pub fn reset(&mut self) {
        *self = LastRecentlyUsedList::new(self.max_space_on_disc);
    }

    /// Takes the entry and moves it to the front. Returns if we need to save the structure.
    /// User has to make sure that the data is contained in the table.
    pub fn touch(&mut self, data: u64) {
        let index = *self.forward_map.get(&data).expect("Data is not in lru list.");
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


    /// Generates a new entry at the top,  we also return the files that need to get deleted.
    pub fn insert_and_clear(&mut self, data: u64, space_on_disc: u64) -> Vec<u64> {
        debug_assert!(space_on_disc > 0, "There should be no empty files");
        self.space_on_disc += space_on_disc;
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
        element.disc_space_consumption = space_on_disc;
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

        self.fit_in_budget()
    }

    /// Makes the system fit into the budget and returns a list of files to be deleted.
    fn fit_in_budget(&mut self) -> Vec<u64> {
        // Eventually we have to clean up some data.
        let mut free_list = Vec::new();
        while let Some(scan) = self.last_entry
            && (self.space_on_disc > self.max_space_on_disc)
        {
            let element = &self.entry_list[scan as usize];
            let freed_amount = element.disc_space_consumption;
            debug_assert!(freed_amount > 0, "We should have no orphan files in the list.");
            let content = element.data;
            self.forward_map.remove(&content);
            free_list.push(content);
            self.open_spaces.push(scan);
            self.last_entry = get_u32(element.previous);
            if let Some(previous) = self.last_entry {
                self.entry_list[previous as usize].next = None;
            }
            self.space_on_disc -= freed_amount;
        }
        // Check if we have flushed all.
        if self.last_entry.is_none() {
            self.first_entry = None;
        }

        free_list
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
    fn get_lru_entry(index: u32, last_element: u32, content: u64, disc_space_consumption: u64) -> LRUEntry {
        LRUEntry {
            previous: (index > 0).then(|| set_u32(index - 1).unwrap()),
            next: (index < last_element).then(|| set_u32(index + 1).unwrap()),
            data: content,
            disc_space_consumption
        }
    }

    /// Generates an LRU eviction system from an u64 slice handed over. This method
    /// is meant as an entry point for loading. We want to get all elements of the
    /// lru content, that has also a file handle in lodaded handles and then add the handles in
    /// loaded handles, that are not in lru_content. So we have exactly the loaded_file_handles in there
    /// in the end just in different order. Also we return a list of files to be deleted to fit into the budget.
    pub fn reconstruct_from(&mut self, lru_content: &[u64], existing_entries: &[TileData]) -> Vec<u64> {
        self.reset();
        if  existing_entries.is_empty() {
            return Vec::new();
        }
        self.space_on_disc  = existing_entries.iter().map(|x| x.size_on_disc).sum();

        let total_size = existing_entries.len();
        debug_assert!(self.space_on_disc > 0, "As we have files we should have a larger file size.");
        let last_element = total_size as u32 - 1;
        self.forward_map.reserve(total_size);
        let mut sequence_content: Vec<LRUEntry> = Vec::with_capacity(total_size);

        let mut look_up_for_existing = existing_entries.iter().map(|x| (x.tile_id, x.size_on_disc)).collect::<FxHashMap<u64, u64>>();

        debug_assert_eq!(
            look_up_for_existing.len(),
            total_size,
            "The loaded files should be unique per construction"
        );

        let mut position_counter = 0;
        // First pass we go over the lru content.
        for &x in lru_content {
            // Skip all entries that are not loaded.
            let Some(file_size) = look_up_for_existing.remove(&x) else {continue;};
            sequence_content.push(Self::get_lru_entry(position_counter, last_element, x, file_size));
            self.forward_map.insert(x, position_counter);
            position_counter += 1;
        }
        // Now we add all the remaining files, that are loaded but not in the content table.
        for (data, size) in look_up_for_existing.into_iter() {
            sequence_content.push(Self::get_lru_entry(position_counter, last_element, data, size));
            self.forward_map.insert(data, position_counter);
            position_counter += 1;
        }

        debug_assert_eq!(sequence_content.len(), total_size);
        debug_assert_eq!(position_counter, total_size as u32);

        self.entry_list = sequence_content;
        self.first_entry = Some(0);
        self.last_entry = Some(last_element);

        return self.fit_in_budget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_test() {
        let cand = LastRecentlyUsedList::new(20);
        assert_eq!(cand.generate_usage_list(), Vec::new());
    }

    #[test]
    fn filter_test() {
        let raw_data = vec![0, 1, 2, 3, 4, 5];
        let test_data = vec![5,2,3,22];
        let file_data = test_data.iter().map(|x| TileData{tile_id: *x, size_on_disc: 1}).collect::<Vec<_>>();
        let expected_data = vec![2, 3, 5, 22];
        let mut cand = LastRecentlyUsedList::new(20);
        cand.reconstruct_from(&raw_data, &file_data);
        assert_eq!(cand.generate_usage_list(), expected_data);
    }

    #[test]
    fn fill_test() {
        let mut cand = LastRecentlyUsedList::new(20);
        let total_vec = vec![0, 1, 2, 3, 4];
        let entry_vec = total_vec.iter().map(|x| TileData{tile_id: *x, size_on_disc: 1}).collect::<Vec<_>>();
        for i in total_vec.iter().rev() {
            cand.insert_and_clear(*i, 1);
        }

        let usage_list = cand.generate_usage_list();

        assert_eq!(cand.generate_usage_list(), vec![0, 1, 2, 3, 4]);
        cand.touch(0);
        assert_eq!(cand.generate_usage_list(), vec![0, 1, 2, 3, 4]);
        cand.touch(4);
        assert_eq!(cand.generate_usage_list(), vec![4, 0, 1, 2, 3]);
        cand.reconstruct_from(&usage_list, &entry_vec);
        cand.touch(2);
        assert_eq!(cand.generate_usage_list(), vec![2, 0, 1, 3, 4]);
    }

    #[test]
    fn reconstruct_test() {
        let mut cand = LastRecentlyUsedList::new(20);
        let total_vec = vec![0, 1, 2, 3, 4,5];
        let input_vec = total_vec.iter().map(|x| TileData{tile_id: *x, size_on_disc: 1}).collect::<Vec<_>>();

        cand.reconstruct_from(&[], &input_vec);
        let mut list = cand.generate_usage_list();
        list.sort();
        assert_eq!(list, total_vec);
    }
}
