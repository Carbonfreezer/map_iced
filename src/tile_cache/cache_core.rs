//! This is the rea core of the tile cache.
//! It provides asynchronous access to the the caching system.

use crate::tile_cache::file_util::{FileUtil, round_to_final_consumption};
use crate::tile_cache::lru_list::LastRecentlyUsedList;
use crate::tile_cache::tile_name_conversion::TileSpecification;
use crate::tile_cache::web_requester::{DummyRequester, Requester, WebRequester};
use fxhash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{mpsc};

///  The maximum channel size we currently allow for.
const MAXIMUM_MESSAGE_CHANNEL: usize = 100;

/// The filename for the LRU table.
const LRU_TABLE_FILE: &str = "LRU.bin";

/// This struct contains the elements that must be shared across different tasks.
struct ShareableEntries<T: Requester> {
    /// The file utility for file access.
    file_util: FileUtil,
    /// The lru list behind a mutex.
    lru_list: tokio::sync::Mutex<LastRecentlyUsedList>,
    /// The amount of data we currently administer.
    amount_of_data: AtomicU64,
    /// The maximum data we currently allow for.
    maximum_amount_of_data: u64,
    /// The requester we use for making web requests.
    requester: T,
    /// Flags that the initialization is completed.
    initialization_completed: AtomicBool,
    /// System to avoid double loading of the same item several times.
    loading_set: std::sync::Mutex<FxHashSet<u64>>,
}

/// The different messages that come from the caching system,
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CachingResultMessage {
    /// We have encountered an error, message included.
    Error { message: String },
    /// The initialization request has been competed.
    InitializationCompleted,
    /// The tile data, that has been retrieved.
    TileData {
        /// Zoom level.
        level: u8,
        /// x coordinate.
        x: u32,
        /// y coordinate.
        y: u32,
        /// The data of the tile.
        data: Vec<u8>,
    },
}

/// The caching system we also represent to the outside.
pub struct CachingSystem<T: Requester> {
    /// The entry that gets cloned for every working thread.
    cloneable_entry: Arc<ShareableEntries<T>>,
    /// The stream reader for the tokio stream.
    stream_reader: Receiver<CachingResultMessage>,
    /// The sender used for data,
    stream_sender: Sender<CachingResultMessage>,
}

impl<T: Requester> CachingSystem<T> {
    pub fn new(
        requester: T,
        cache_base_dir: impl AsRef<Path>,
        maximum_amount_of_data: u64,
    ) -> CachingSystem<T> {
        let (tx, rx) = mpsc::channel::<CachingResultMessage>(MAXIMUM_MESSAGE_CHANNEL);
        let sharable_entry = ShareableEntries {
            file_util: FileUtil::new(cache_base_dir),
            lru_list: tokio::sync::Mutex::new(LastRecentlyUsedList::default()),
            amount_of_data: AtomicU64::new(0),
            maximum_amount_of_data,
            requester,
            initialization_completed: AtomicBool::new(false),
            loading_set: std::sync::Mutex::new(FxHashSet::default()),
        };

        Self {
            cloneable_entry: Arc::new(sharable_entry),
            stream_reader: rx,
            stream_sender: tx,
        }
    }

    /// Polls the resulting que.
    pub async fn poll_result(&mut self) -> CachingResultMessage {
        self.stream_reader
            .recv()
            .await
            .expect("CachingSystem::pol_result last sender was dropped, should actually not happen")
    }

    /// Asynchronous initialization function to set up the system.
    async fn process_initialize(
        sharable_entry: Arc<ShareableEntries<T>>,
        sender: Sender<CachingResultMessage>,
    ) {
        // Remove all orphan files.
        sharable_entry.file_util.remove_temps_from_base().await;
        // Let us check if we get an lru table.
        if let Some(load_data) = sharable_entry
            .file_util
            .try_load_plain(LRU_TABLE_FILE)
            .await
        {
            sharable_entry
                .lru_list
                .lock()
                .await
                .reconstruct_from(&FileUtil::convert_to_u64(&load_data));
        }

        // Now we load all the data from the images on the cache.
        let data = sharable_entry
            .file_util
            .get_all_pngs_interpreted_as_u64()
            .await;
        // Check the LRU List if all is contained.
        sharable_entry
            .lru_list
            .lock()
            .await
            .complete_list(&data.tile_ids);
        // Eventually we have to deal with an oversized cache.
        if data.total_file_size > sharable_entry.maximum_amount_of_data {
            let amount_to_free = data.total_file_size - sharable_entry.maximum_amount_of_data;
            let clear_data = sharable_entry
                .lru_list
                .lock()
                .await
                .free_elements(amount_to_free, &sharable_entry.file_util)
                .await;
            // Remove the files.
            sharable_entry
                .file_util
                .remove_files(&clear_data.tile_ids)
                .await;
            // Store the result.
            sharable_entry.amount_of_data.store(
                data.total_file_size - clear_data.total_file_size,
                Ordering::Relaxed,
            );
        } else {
            // Set the required data.
            sharable_entry
                .amount_of_data
                .store(data.total_file_size, Ordering::Relaxed);
        }

        sharable_entry
            .initialization_completed
            .store(true, Ordering::Relaxed);

        // If an error occurred the receiver has been dropped in the meantime.
        let _ = sender
            .send(CachingResultMessage::InitializationCompleted)
            .await;
    }

    /// Initializes the system. Starts an internal tokio task for the work.
    pub fn initialize(&mut self) -> Result<(), String> {
        if self
            .cloneable_entry
            .initialization_completed
            .load(Ordering::Relaxed)
        {
            return Err(String::from(
                "CachingSystem::initialize already initialized",
            ));
        }
        let shareable_entry = self.cloneable_entry.clone();
        let sender = self.stream_sender.clone();
        tokio::spawn(Self::process_initialize(shareable_entry, sender));
        Ok(())
    }

    /// Poses a request for a tile. Starts an internal tokio task for the work.
    pub fn request_tile(&self, level: u8, x: u32, y: u32) -> Result<(), String> {
        if !self
            .cloneable_entry
            .initialization_completed
            .load(Ordering::Relaxed)
        {
            return Err(String::from(
                "CachingSystem::request_tile not initialized yet",
            ));
        }
        let shareable_entry = self.cloneable_entry.clone();
        let sender = self.stream_sender.clone();
        tokio::spawn(Self::process_request_tile(
            level,
            x,
            y,
            shareable_entry,
            sender,
        ));
        Ok(())
    }

    /// Internal work for requesting a tile basically distinguishes between cache hit and miss.
    async fn process_request_tile(
        level: u8,
        x: u32,
        y: u32,
        sharable_entry: Arc<ShareableEntries<T>>,
        sender: Sender<CachingResultMessage>,
    ) {
        let destination = TileSpecification::new(level, x, y);
        // Here we have to distinguish if we have it on cache or not.
        if let Some(image_data) = sharable_entry
            .file_util
            .try_load_plain(destination.filename())
            .await
        {
            // First send off the result.
            let _ = sender
                .send(CachingResultMessage::TileData {
                    level,
                    x,
                    y,
                    data: image_data,
                })
                .await;
            // Now we have to check with the cache.
            if sharable_entry
                .lru_list
                .lock()
                .await
                .touch_or_insert(destination.into())
            {
                Self::save_lru_table(&sharable_entry, sender).await;
            }
        } else {
            Self::deal_with_cache_miss(&sharable_entry, sender, destination).await;
        }
    }

    /// The cache miss part is more complicated but tries to serve the data as fast as possible.
    async fn deal_with_cache_miss(
        sharable_entry: &Arc<ShareableEntries<T>>,
        sender: Sender<CachingResultMessage>,
        destination: TileSpecification,
    ) {
        let current_request_id = u64::from(destination);
        // First we check if we already have the element in process.
        if !sharable_entry
            .loading_set
            .lock()
            .expect("Poisoned")
            .insert(current_request_id)
        {
            return;
        }
        // In this case the file is not on the cache so we have to get it.
        let web_access = sharable_entry.requester.get_image_data(destination).await;
        let raw_data = match web_access {
            Ok(data) => data,
            Err(text) => {
                let _ = sender
                    .send(CachingResultMessage::Error { message: text })
                    .await;
                sharable_entry
                    .loading_set
                    .lock()
                    .expect("Poisoned")
                    .remove(&current_request_id);
                return;
            }
        };

        // First send the result.
        let _ = sender
            .send(CachingResultMessage::TileData {
                level: destination.level(),
                x: destination.x(),
                y: destination.y(),
                data: raw_data.clone(),
            })
            .await;

        let mut new_memory = round_to_final_consumption(raw_data.len() as u64);
        // Now we have to save the data on the disc.
        if let Err(text) = sharable_entry
            .file_util
            .safe_save(destination.filename(), &raw_data)
            .await
        {
            let _ = sender
                .send(CachingResultMessage::Error { message: text })
                .await;

            new_memory = 0;
        }

        let total_memory = sharable_entry.amount_of_data.load(Ordering::SeqCst) + new_memory;

        // First we append ourselves
        sharable_entry
            .lru_list
            .lock()
            .await
            .touch_or_insert(destination.into());

        // If we have run over budget, we have to eliminate entries.
        let subtracted_memory = if total_memory > sharable_entry.maximum_amount_of_data {
            let clean_result = sharable_entry
                .lru_list
                .lock()
                .await
                .free_elements(
                    total_memory - sharable_entry.maximum_amount_of_data,
                    &sharable_entry.file_util,
                )
                .await;
            // Remove files.
            sharable_entry
                .file_util
                .remove_files(&clean_result.tile_ids)
                .await;
            clean_result.total_file_size
        } else {
            0
        };

        // Now set the new memory.
        if new_memory > subtracted_memory {
            sharable_entry
                .amount_of_data
                .fetch_add(new_memory - subtracted_memory, Ordering::SeqCst);
        } else {
            sharable_entry
                .amount_of_data
                .fetch_sub(subtracted_memory - new_memory, Ordering::SeqCst);
        }

        // Release the book keeping.
        sharable_entry
            .loading_set
            .lock()
            .expect("Poisoned")
            .remove(&current_request_id);

        // Save the new table.
        Self::save_lru_table(sharable_entry, sender).await;
    }

    /// Helper routine to write out the cache file.
    async fn save_lru_table(
        sharable_entry: &Arc<ShareableEntries<T>>,
        sender: Sender<CachingResultMessage>,
    ) {
        let raw_data = sharable_entry.lru_list.lock().await.generate_usage_list();
        let res = sharable_entry
            .file_util
            .safe_save(LRU_TABLE_FILE, &FileUtil::convert_to_u8(&raw_data))
            .await;
        match res {
            Ok(_) => {} // Nothing to do here.
            Err(x) => {
                let _ = sender
                    .send(CachingResultMessage::Error {
                        message: "Error on writing cache file: ".to_string() + x.as_str(),
                    })
                    .await;
            }
        }
    }
}

/// The dummy cache we use for testing purposes.
pub fn generate_dummy_cache(
    cache_base_dir: impl AsRef<Path>,
    maximum_amount_of_data: u64,
) -> CachingSystem<DummyRequester> {
    CachingSystem::new(DummyRequester, cache_base_dir, maximum_amount_of_data)
}

/// Generates the real cache. The first 3 entries refer to the url and the username to access the web service.
/// Cache base directory is the place where we store data and the maximum amount is the maximum amount of data we want to take.
pub fn generate_cache(
    intro_url: &str,
    post_url: &str,
    user_agent: &str,
    cache_base_dir: impl AsRef<Path>,
    maximum_amount_of_data: u64,
) -> CachingSystem<WebRequester> {
    CachingSystem::new(
        WebRequester::new(intro_url, post_url, user_agent),
        cache_base_dir,
        maximum_amount_of_data,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;
    use std::time::Duration;

    #[tokio::test]
    async fn first_setup() {
        let mut cache = generate_dummy_cache(tempfile::tempdir().unwrap(), 10_000);
        cache.initialize().expect("Already initialized.");
        let message = cache.poll_result().await;
        assert_eq!(message, CachingResultMessage::InitializationCompleted);
        // Hack to make sure the data is on disc.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn first_fill() {
        let mut cache = generate_dummy_cache(tempfile::tempdir().unwrap(), 20_000);
        cache.initialize().expect("Already initialized.");
        let message = cache.poll_result().await;
        assert_eq!(message, CachingResultMessage::InitializationCompleted);
        for x in 0..100 {
            cache
                .request_tile(0, x, 1)
                .expect("Initialization uncompleted.");
            cache
                .request_tile(0, x, 1)
                .expect("Initialization uncompleted.");
        }

        for _ in 0..100 {
            let message = cache.poll_result().await;
            assert_matches!(
                message,
                CachingResultMessage::TileData { level: 0, y: 1, .. }
            );
        }
        // Hack to make sure the data is on disc.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
