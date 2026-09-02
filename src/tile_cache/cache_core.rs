//! This is the rea core of the tile cache.
//! It provides asynchronous access to the the caching system.
//!
//! It is the responsibility of the user to make sure that no two requests for the same
//! tile type are in flight at the same time.

use crate::tile_cache::file_util::{FileUtil, round_to_final_consumption};
use crate::tile_cache::lru_list::LastRecentlyUsedList;
use crate::tile_cache::tile_name_conversion::TileSpecification;
use crate::tile_cache::web_requester::{DummyRequester, Requester, WebRequester};
use bytes::Bytes;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

///  The maximum channel size we currently allow for.
const MAXIMUM_MESSAGE_CHANNEL: usize = 100;

/// The filename for the LRU table.
const LRU_TABLE_FILE: &str = "LRU.bin";

/// The amount of idle seconds we use till saving.
const AMOUNT_OF_SECONDS_TILL_SAVE: u8 = 5;

/// This struct contains the elements that must be shared across different tasks.
struct ShareableEntries<T: Requester> {
    /// The file utility for file access.
    file_util: FileUtil,
    /// The lru list behind a mutex.
    lru_list: Mutex<LastRecentlyUsedList>,
    /// The requester we use for making web requests.
    requester: T,
    /// Flags that the initialization is completed.
    initialization_completed: AtomicBool,
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
        /// The data of the tile, put behind an arc to prevent expensive cloning.
        data: Bytes,
    },
    /// Contains the information that a tile id has failed, needed  for book keeping.
    TileFailed {
        /// Zoom level.
        level: u8,
        /// x coordinate.
        x: u32,
        /// y coordinate.
        y: u32,
        /// Why did the tile not arrive..
        message: String,
    },
}

/// The caching system we also represent to the outside.
pub struct CachingSystem<T: Requester> {
    /// The entry that gets cloned for every working thread.
    cloneable_entry: Arc<ShareableEntries<T>>,
    /// The stream reader for the tokio stream.
    stream_reader: Mutex<Option<Receiver<CachingResultMessage>>>,
    /// The sender used for data,
    stream_sender: Sender<CachingResultMessage>,
    /// The atomic counter we use for saving the lru cache.
    savings_counter: Arc<AtomicU8>,
    /// The join handle for the timer to kill in drop trait.
    timer_handle: JoinHandle<()>,
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
            lru_list: Mutex::new(LastRecentlyUsedList::new(maximum_amount_of_data)),
            requester,
            initialization_completed: AtomicBool::new(false),
        };

        let arc_sharable_entry = Arc::new(sharable_entry);
        let savings_counter = Arc::new(AtomicU8::new(0));
        let timer_handle = tokio::spawn(Self::savings_timer(
            savings_counter.clone(),
            arc_sharable_entry.clone(),
            tx.clone(),
        ));

        Self {
            cloneable_entry: arc_sharable_entry,
            stream_reader: Mutex::new(Some(rx)),
            stream_sender: tx,
            savings_counter,
            timer_handle,
        }
    }

    /// Timer function that does an autosave after a couple of idle seconds.
    async fn savings_timer(
        counter: Arc<AtomicU8>,
        shareable_entries: Arc<ShareableEntries<T>>,
        sender: Sender<CachingResultMessage>,
    ) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let previous = counter.try_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                Some(current.saturating_sub(1))
            });
            if previous == Ok(1) {
                Self::save_lru_table(&shareable_entries, &sender).await;
            }
        }
    }

    /// Extracts the receiver out of the class. Can only be done one time.
    pub fn get_receiver(&self) -> Option<Receiver<CachingResultMessage>> {
        self.stream_reader.lock().unwrap().take()
    }

    /// Asynchronous initialization function to set up the system.
    async fn process_initialize(
        sharable_entry: Arc<ShareableEntries<T>>,
        sender: Sender<CachingResultMessage>,
        savings_counter: Arc<AtomicU8>,
    ) {
        // Remove all orphan files.
        sharable_entry.file_util.remove_temps_from_base().await;
        // Now we load all the data from the images on the cache.
        let data = sharable_entry
            .file_util
            .get_all_pngs_interpreted_as_u64()
            .await;
        // Let us check if we get an lru table.
        let lru_data = FileUtil::convert_to_u64(
            &sharable_entry
                .file_util
                .try_load_plain(LRU_TABLE_FILE)
                .await
                .unwrap_or_default(),
        );
        let deletion_list = sharable_entry
            .lru_list
            .lock()
            .unwrap()
            .reconstruct_from(&lru_data, &data);

        // Remove any orphan files, we do not cache anymore.
        sharable_entry
            .file_util
            .remove_files(&deletion_list)
            .await;

        sharable_entry
            .initialization_completed
            .store(true, Ordering::Relaxed);

        // We also register a save because the contents of the LRU may have changed.
        savings_counter.store(AMOUNT_OF_SECONDS_TILL_SAVE, Ordering::SeqCst);

        // If an error occurred the receiver has been dropped in the meantime.
        let _ = sender
            .send(CachingResultMessage::InitializationCompleted)
            .await;
    }

    /// Initializes the system. Starts an internal tokio task for the work.
    pub fn initialize(&self) -> Result<(), String> {
        if self
            .cloneable_entry
            .initialization_completed
            .load(Ordering::Relaxed)
        {
            return Err(String::from(
                "CachingSystem::initialize already initialized",
            ));
        }
        tokio::spawn(Self::process_initialize(
            self.cloneable_entry.clone(),
            self.stream_sender.clone(),
            self.savings_counter.clone(),
        ));
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
            self.savings_counter.clone(),
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
        savings_counter: Arc<AtomicU8>,
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
                    data: Bytes::from(image_data),
                })
                .await;
            // Now we have to check with the cache.
            sharable_entry
                .lru_list
                .lock()
                .unwrap()
                .touch(destination.into());
        } else {
            Self::deal_with_cache_miss(&sharable_entry, sender, destination).await;
        }
        // Flag for cache flush.
        savings_counter.store(AMOUNT_OF_SECONDS_TILL_SAVE, Ordering::SeqCst);
    }

    /// The cache miss part is more complicated but tries to serve the data as fast as possible.
    async fn deal_with_cache_miss(
        sharable_entry: &Arc<ShareableEntries<T>>,
        sender: Sender<CachingResultMessage>,
        destination: TileSpecification,
    ) {
        // In this case the file is not on the cache so we have to get it.
        let web_access = sharable_entry.requester.get_image_data(destination).await;
        let raw_data: Bytes = match web_access {
            Ok(data) => Bytes::from(data),
            Err(text) => {
                let _ = sender
                    .send(CachingResultMessage::TileFailed {
                        x: destination.x(),
                        y: destination.y(),
                        level: destination.level(),
                        message: text,
                    })
                    .await;
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

        let new_memory = round_to_final_consumption(raw_data.len() as u64);
        // Now we have to save the data on the disc.
        if let Err(text) = sharable_entry
            .file_util
            .safe_save(destination.filename(), &raw_data)
            .await
        {
            let _ = sender
                .send(CachingResultMessage::Error { message: text })
                .await;

            return;
        }

        let to_remove = sharable_entry.lru_list.lock().unwrap().insert_and_clear(destination.into(), new_memory);
        sharable_entry.file_util.remove_files(&to_remove).await;

    }

    /// Helper routine to write out the cache file.
    async fn save_lru_table(
        sharable_entry: &Arc<ShareableEntries<T>>,
        sender: &Sender<CachingResultMessage>,
    ) {
        let raw_data = sharable_entry.lru_list.lock().unwrap().generate_usage_list();
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

impl<T: Requester> Drop for CachingSystem<T> {
    fn drop(&mut self) {
        self.timer_handle.abort();
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
    // #[ignore]
    async fn first_setup() {
        let cache = generate_dummy_cache(tempfile::tempdir().unwrap(), 10_000);
        // let cache = generate_dummy_cache("transient", 20_000);
        cache.initialize().expect("Already initialized.");
        let mut receiver = cache.get_receiver().unwrap();
        let message = receiver.recv().await.unwrap();
        assert_eq!(message, CachingResultMessage::InitializationCompleted);
        // tokio::time::sleep(Duration::from_secs(AMOUNT_OF_SECONDS_TILL_SAVE as u64 + 2)).await;
    }

    #[tokio::test]
    async fn first_fill() {
        let cache = generate_dummy_cache(tempfile::tempdir().unwrap(), 20_000);
        // let cache = generate_dummy_cache("transient", 20_000);
        cache.initialize().expect("Already initialized.");
        let mut receiver = cache.get_receiver().unwrap();
        let message = receiver.recv().await.unwrap();
        assert_eq!(message, CachingResultMessage::InitializationCompleted);
        for y in 0..100 {
            cache
                .request_tile(0,  1, y)
                .expect("Initialization uncompleted.");
        }


        for _ in 0..100 {
            let message = receiver.recv().await.unwrap();
            assert_matches!(
                message,
                CachingResultMessage::TileData { level: 0, x: 1, .. }
            );
        }
        // Wait for the data written to disc.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
