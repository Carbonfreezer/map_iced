//! This is the rea core of the tile cache.
//! It provides asynchronous access to the the caching system.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64};
use tokio::sync::mpsc::{Receiver,Sender};
use tokio::sync::{Mutex, mpsc};
use crate::tile_cache::file_util::FileUtil;
use crate::tile_cache::lru_list::LastRecentlyUsedList;
use crate::tile_cache::web_requester::Requester;

///  The maximum channel size we currently allow for.
const MAXIMUM_MESSAGE_CHANNEL : usize = 100;

/// This struct contains the elements that must be shared across different tasks.
struct ShareableEntries<T : Requester> {
    /// The file utility for file access.
    file_util : FileUtil,
    /// The lru list behind a mutex.
    lru_list: Mutex<LastRecentlyUsedList>,
    /// The amount of data we currently administer.
    amount_of_data : AtomicU64,
    /// The maximum data we currently allow for.
    maximum_amount_of_data : u64,
    /// The requester we use for making web requests.
    requester: T,
}

/// The different messages that come from the caching system,
pub enum CachingResultMessage {
    /// We have encountered an error, message included.
    Error{message: String},
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
        data: Vec<u8>
    }
}

/// The caching system we also represent to the outside.
pub struct CachingSystem<T : Requester> {
    /// The entry that gets cloned for every working thread.
    cloneable_entry: Arc<ShareableEntries<T>>,
    /// The stream reader for the tokio stream.
    stream_reader : Receiver<CachingResultMessage>,
    /// The sender used for data,
    stream_sender : Sender<CachingResultMessage>,
    /// Flags that we are initialized.
    is_initialized : bool,
}

impl<T : Requester> CachingSystem<T> {
    pub fn new(requester : T, cache_base_dir : impl AsRef<Path>, maximum_amount_of_data : u64) -> CachingSystem<T> {
        let (tx,rx) = mpsc::channel::<CachingResultMessage>(MAXIMUM_MESSAGE_CHANNEL);
        let sharable_entry = ShareableEntries {
            file_util : FileUtil::new(cache_base_dir),
            lru_list : Mutex::new(LastRecentlyUsedList::default()),
            amount_of_data : AtomicU64::new(0),
            maximum_amount_of_data,
            requester,
        };

        Self {
            cloneable_entry : Arc::new(sharable_entry),
            stream_reader : rx,
            stream_sender : tx,
            is_initialized: false,
        }
    }


    /// Polls the resulting que.
    pub async fn poll_result(&mut self) -> CachingResultMessage {
        self.stream_reader.recv().await.expect("CachingSystem::pol_result last sender was dropped, should actually not happen")
    }

    async fn process_initialize(sharable_entry: Arc<ShareableEntries<T>>, sender : Sender<CachingResultMessage>) {

    }

    /// Initializes the system.
    pub fn initialize(&mut self) {
        assert!(!self.is_initialized, "We should be unitialzed");
        self.is_initialized = true;
        let shareable_entry = self.cloneable_entry.clone();
        let sender = self.stream_sender.clone();
        tokio::spawn(Self::process_initialize(shareable_entry, sender));
    }
}