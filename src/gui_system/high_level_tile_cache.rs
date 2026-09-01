//! This module administrates the iced image handles for potentially several different widgets.

use std::mem::take;
use fxhash::{FxHashMap, FxHashSet};
use iced::advanced::graphics::core::Bytes;
use iced::advanced::image::Handle;
use tokio::sync::mpsc::Receiver;
use crate::gui_system::math_coordinates::{BoundingRectangle, TileChange, TilePosition};
use crate::tile_cache::cache_core::{CachingResultMessage, CachingSystem};
use crate::tile_cache::web_requester::Requester;


/// Contains the image entry.
#[derive(Debug, Clone)]
struct ImageEntry {
    /// The real image if already existing.
    image: Option<Handle>,
    /// The counter how often the image is used.
    usage_counter: u32
}

/// Contains the info the client is currently subscribed for.
#[derive(Debug, Clone, Copy)]
struct ClientSubscriptionRecord {
    area: BoundingRectangle,
    zoom: u8
}

/// The update messages that can be drained from the system.
#[derive(Clone, Debug)]
pub enum UpdateMessage {
    /// An internal error in the caching system has occurred and can be obtained here.
    ErrorMessage {text: String},
    /// A new tile has arrived that the indicated client is waiting for.
    RelevantTilesArrived {client: u32},
}


pub struct TileCache<T : Requester> {
    /// The core we have access too.
    core : CachingSystem<T>,
    /// This stores the requests and delays delivery until the system is initialized.
    pending_requests : Vec<TilePosition>,
    /// Stores the currently associated image for a certain tile coordinate. Contains the image and a usage counter.
    content_tiles : FxHashMap<TilePosition, ImageEntry>,
    /// The subscription list which client has subscribed for which region.
    subscription_region : FxHashMap<u32, ClientSubscriptionRecord>,
    /// Stores, if we have passed the initialization phase.
    is_initialized : bool,
    /// The accumulated error string.
    error_msg : String,
    /// The hash set of clients, whose subscription got affected.
    client_notifications : FxHashSet<u32>
}

impl<T : Requester> TileCache<T> {

    /// Creates a tile cache from the low level system handed over.
    pub fn new(requestor : CachingSystem<T>) -> Result<Self, String> {
        // Kick off the initialization process.
        requestor.initialize()?;
        Ok (Self {
            core : requestor,
            pending_requests : Vec::new(),
            content_tiles : FxHashMap::default(),
            subscription_region : FxHashMap::default(),
            is_initialized : false,
            error_msg : "".to_string(),
            client_notifications : FxHashSet::default(),
        })
    }

    /// Extracts the receiver out of the class. Can only be done one time.
    pub fn get_receiver(&self) -> Option<Receiver<CachingResultMessage>> {
        self.core.get_receiver()
    }

    /// Gets the internal update messages that have been accumulated,
    pub fn drain_result_messages(&mut self) -> Vec<UpdateMessage> {
        let mut result : Vec<UpdateMessage> = Vec::new();
        if !self.error_msg.is_empty() {
            result.push(UpdateMessage::ErrorMessage {text: self.error_msg.trim().into()});
        }
        for &client in &self.client_notifications {
            result.push(UpdateMessage::RelevantTilesArrived {client});
        }
        self.error_msg = "".to_string();
        self.client_notifications.clear();
        result
    }


    /// Gets called to process the messages, effectively those that have been pulled out the receiver
    pub fn process_caching_message(&mut self, message: CachingResultMessage) {
        match message {
            CachingResultMessage::Error { message: text } => {self.error_msg += &*(text + "\n");},
            CachingResultMessage::InitializationCompleted => {self.is_initialized = true; self.flush_pending_requests()},
            CachingResultMessage::TileData { x, y, level, data} => {
                let pos = TilePosition{x,y,zoom: level};
                // Eventually we have a late arriver nobody is interested in anymore.
                let Some(cache_entry) = self.content_tiles.get_mut(&pos) else {return;};
                debug_assert!(cache_entry.image.is_none(), "The image should be empty now");
                cache_entry.image = Some(Handle::from_bytes(Bytes::copy_from_slice(&data)));
                for (client, region) in &self.subscription_region {
                    if region.zoom == level && region.area.contains_position(&pos){
                        self.client_notifications.insert(*client);
                    }
                }

            }
        }

    }

    /// The client present wants to completely unsubscribe.
    pub fn completely_unsubscribe(&mut self, client : u32) {
        let Some(region) = self.subscription_region.remove(&client) else {return};

        for x in region.area.get_iterator(region.zoom) {
            self.decrement_usage(&x);
        }
    }

    /// We register for a new entrance area.
    pub fn register_new_interest_area(&mut self, client : u32, area : BoundingRectangle, zoom : u8) {
        let mut tile_change = TileChange::default();
        // Do we already have something?
        if let Some(old_area) = self.subscription_region.remove(&client) {
            // See if we have switched levels.
            if old_area.zoom != zoom {
                tile_change.deleted = old_area.area.get_iterator(old_area.zoom).collect();
                tile_change.added = area.get_iterator(zoom).collect();
            } else {
                tile_change =  old_area.area.generate_deletion_creation_list(&area, zoom);
            }
        } else {
            // This is a new region completely register it.
            tile_change.added =  area.get_iterator(zoom).collect();
        }
        for pos in tile_change.added {
            self.increment_usage(&pos);
        }
        for pos in tile_change.deleted {
            self.decrement_usage(&pos);
        }

        self.subscription_region.insert(client, ClientSubscriptionRecord { area, zoom});
    }

    /// Tries to get the image from the store.
    pub fn try_get_image(&self, position : &TilePosition) -> Option<Handle> {
        self.content_tiles.get(position).and_then(|entry| entry.image.clone())
    }

    /// Deals with the fact, that someone stops using a specific tile, eventually it will get deleted.
    fn decrement_usage(&mut self, position : &TilePosition) {
        // Get the entry, here we also take care of, that it eventually does not exist.
        let Some(entry) = self.content_tiles.get_mut(position) else {return;};
        entry.usage_counter = entry.usage_counter.saturating_sub(1);
        if entry.usage_counter == 0 {
            self.content_tiles.remove(position);
        }
    }

    /// Increments the usage count, if it does not exist yet, we register it and send the delivery request.
    fn increment_usage(&mut self, position : &TilePosition) {
        if let Some(entry) = self.content_tiles.get_mut(position) {
            entry.usage_counter += 1;
        } else {
            // Here we have a new position.
            self.content_tiles.insert(*position, ImageEntry {image: None, usage_counter: 1});
            if self.is_initialized {
                self.core.request_tile(position.zoom, position.x, position.y).expect("Inconsistent tile initialization state");
            } else {
                self.pending_requests.push(*position);
            }
        }
    }

    /// Flushes all pending requests.
    fn flush_pending_requests(&mut self) {
        for p in take(&mut self.pending_requests) {
            self.core.request_tile(p.zoom, p.x, p.y).expect("Inconsistent start state of tile cache.");
        };
    }

}