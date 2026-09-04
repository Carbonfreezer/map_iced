//! This module administrates the iced image handles for potentially several different widgets.

use crate::gui_system::math_coordinates::{BoundingRectangle, TileChange, TilePosition};
use crate::tile_cache::cache_core::{CachingResultMessage, CachingSystem};
use crate::tile_cache::web_requester::Requester;
use fxhash::{FxHashMap, FxHashSet};
use iced::advanced::image::Handle;
use std::mem::take;
use tokio::sync::mpsc::Receiver;

/// Contains the image entry.
#[derive(Debug, Clone)]
struct ImageEntry {
    /// The real image if already existing.
    image: Option<Handle>,
    /// The counter how often the image is used.
    usage_counter: u32,
}

/// The update messages that can be drained from the system.
#[derive(Clone, Debug)]
pub enum CacheUpdateMessage {
    /// An internal error in the caching system has occurred and can be obtained here.
    ErrorMessage { text: String },
    /// A new tile has arrived that the indicated client is waiting for.
    RelevantTilesArrived { client: u32 },
}

/// The tiles to draw as a  result for a specific client.
pub struct TilesToDraw {
    pub position: TilePosition,
    pub image: Handle,
}

pub struct TileCache<T: Requester> {
    /// The core we have access too.
    core: CachingSystem<T>,
    /// This stores the requests and delays delivery until the system is initialized.
    pending_requests: Vec<TilePosition>,
    /// Stores the currently associated image for a certain tile coordinate. Contains the image and a usage counter.
    content_tiles: FxHashMap<TilePosition, ImageEntry>,
    /// The subscription list which client has subscribed for which region.
    subscription_region: FxHashMap<u32, BoundingRectangle>,
    /// Stores, if we have passed the initialization phase.
    is_initialized: bool,
    /// The accumulated error string.
    error_msg: String,
    /// Special error message from tile error.
    tile_error_msg: String,
    /// The hash set of clients, whose subscription got affected.
    client_notifications: FxHashSet<u32>,
    /// Tiles for which a request is currently outstanding at the core.
    tiles_in_flight: FxHashSet<TilePosition>,
}

impl<T: Requester> TileCache<T> {
    /// Creates a tile cache from the low level system handed over.
    pub fn new(requestor: CachingSystem<T>) -> Result<Self, String> {
        // Kick off the initialization process.
        requestor.initialize()?;
        Ok(Self {
            core: requestor,
            pending_requests: Vec::new(),
            content_tiles: FxHashMap::default(),
            subscription_region: FxHashMap::default(),
            is_initialized: false,
            error_msg: "".to_string(),
            tile_error_msg: "".to_string(),
            client_notifications: FxHashSet::default(),
            tiles_in_flight: FxHashSet::default(),
        })
    }

    /// Extracts the receiver out of the class. Can only be done one time.
    pub fn get_receiver(&mut self) -> Option<Receiver<CachingResultMessage>> {
        self.core.get_receiver()
    }

    /// Gets the internal update messages that have been accumulated,
    pub fn drain_result_messages(&mut self) -> Vec<CacheUpdateMessage> {
        let mut result: Vec<CacheUpdateMessage> = Vec::new();

        if !self.error_msg.is_empty() || !self.tile_error_msg.is_empty() {
            let mut text = take(&mut self.error_msg);
            text.push_str(&take(&mut self.tile_error_msg));
            result.push(CacheUpdateMessage::ErrorMessage {
                text: text.trim().into(),
            });
        }

        for &client in &self.client_notifications {
            result.push(CacheUpdateMessage::RelevantTilesArrived { client });
        }
        self.client_notifications.clear();
        result
    }

    /// Asks for the  current number of tiles missing can be used for
    /// enabling / disabling buttons.
    pub fn number_of_tiles_failed(&self) -> u32 {
        if !self.is_initialized {
            return 0;
        }
        self.content_tiles
            .iter()
            .filter(|(pos, image)| image.image.is_none() && !self.tiles_in_flight.contains(pos))
            .count() as u32
    }

    /// Gets invoked from the outside to send request for failed tiles again.
    /// A failed tile is a tile with no image data in the cache and whose position is not in tiles in flight.
    pub fn retry_failed_tiles(&mut self) {
        if !self.is_initialized {
            return;
        }

        let missing_pos = self
            .content_tiles
            .iter()
            .filter_map(|(pos, image)| {
                (image.image.is_none() && !self.tiles_in_flight.contains(pos)).then_some(*pos)
            })
            .collect::<Vec<TilePosition>>();

        for pos in missing_pos {
            self.tiles_in_flight.insert(pos);
            self.core
                .request_tile(pos.zoom, pos.x, pos.y)
                .expect("Inconsistent tile initialization state");
        }
    }

    /// Gets called to process the messages, effectively those that have been pulled out the receiver
    pub fn process_caching_message(&mut self, message: CachingResultMessage) {
        match message {
            CachingResultMessage::Error { message: text } => {
                self.error_msg += &*(text + "\n");
            }
            CachingResultMessage::InitializationCompleted => {
                self.is_initialized = true;
                self.flush_pending_requests()
            }
            CachingResultMessage::TileData { x, y, level, data } => {
                let pos = TilePosition { x, y, zoom: level };
                // The request is settled, no matter if anybody is still interested.
                self.tiles_in_flight.remove(&pos);
                // Eventually we have a late arriver nobody is interested in anymore.
                let Some(cache_entry) = self.content_tiles.get_mut(&pos) else {
                    return;
                };
                debug_assert!(cache_entry.image.is_none(), "The image should be empty now");
                cache_entry.image = Some(Handle::from_bytes(data));
                for (client, region) in &self.subscription_region {
                    if region.contains_position(&pos) {
                        self.client_notifications.insert(*client);
                    }
                }
            }
            CachingResultMessage::TileFailed {
                x,
                y,
                level,
                message,
            } => {
                let pos = TilePosition { x, y, zoom: level };
                self.tiles_in_flight.remove(&pos);
                if self.tile_error_msg.is_empty() {
                    self.tile_error_msg = message;
                }
            }
        }
    }

    /// The client present wants to completely unsubscribe.
    pub fn completely_unsubscribe(&mut self, client: u32) {
        let Some(region) = self.subscription_region.remove(&client) else {
            return;
        };

        for x in region.get_iterator() {
            self.decrement_usage(&x);
        }
    }

    /// We register for a new entrance area.
    pub fn register_new_interest_area(&mut self, client: u32, area: BoundingRectangle) {
        let mut tile_change = TileChange::default();
        // Do we already have something?
        if let Some(old_area) = self.subscription_region.remove(&client) {
            tile_change = old_area.generate_deletion_creation_list(&area);
        } else {
            // This is a new region completely register it.
            tile_change.added = area.get_iterator().collect();
        }
        for pos in tile_change.added {
            self.increment_usage(&pos);
        }
        for pos in tile_change.deleted {
            self.decrement_usage(&pos);
        }

        self.subscription_region.insert(client, area);
    }

    /// We get all the images for a specific client we are subscrobed to.
    pub fn get_all_images_for_client(&self, client: u32) -> Vec<TilesToDraw> {
        let Some(bounding_rect) = self.subscription_region.get(&client) else {
            return vec![];
        };
        bounding_rect
            .get_iterator()
            .filter_map(|position| {
                match self
                    .content_tiles
                    .get(&position)
                    .and_then(|image| image.image.clone())
                {
                    Some(image) => Some(TilesToDraw { position, image }),
                    None => None,
                }
            })
            .collect()
    }

    /// Deals with the fact, that someone stops using a specific tile, eventually it will get deleted.
    fn decrement_usage(&mut self, position: &TilePosition) {
        // Get the entry, here we also take care of, that it eventually does not exist.
        let Some(entry) = self.content_tiles.get_mut(position) else {
            return;
        };
        entry.usage_counter = entry.usage_counter.saturating_sub(1);
        if entry.usage_counter == 0 {
            self.content_tiles.remove(position);
        }
    }

    /// Increments the usage count, if it does not exist yet, we register it and send the delivery request.
    fn increment_usage(&mut self, position: &TilePosition) {
        if let Some(entry) = self.content_tiles.get_mut(position) {
            entry.usage_counter += 1;
        } else {
            // Here we have a new position.
            self.content_tiles.insert(
                *position,
                ImageEntry {
                    image: None,
                    usage_counter: 1,
                },
            );
            // Eventually a request from a dropped subscription is still outstanding.
            if !self.tiles_in_flight.insert(*position) {
                return;
            }
            if self.is_initialized {
                self.core
                    .request_tile(position.zoom, position.x, position.y)
                    .expect("Inconsistent tile initialization state");
            } else {
                self.pending_requests.push(*position);
            }
        }
    }

    /// Flushes all pending requests.
    fn flush_pending_requests(&mut self) {
        for p in take(&mut self.pending_requests) {
            self.core
                .request_tile(p.zoom, p.x, p.y)
                .expect("Inconsistent start state of tile cache.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile_cache::cache_core::generate_dummy_cache;
    use itertools::iproduct;
    use std::assert_matches;

    #[tokio::test]
    async fn base_test() {
        const TILE_DIMENSION: u32 = 10;
        let test_rect = BoundingRectangle::new(&[
            TilePosition {
                x: 0,
                y: 0,
                zoom: 5,
            },
            TilePosition {
                x: TILE_DIMENSION - 1,
                y: TILE_DIMENSION - 1,
                zoom: 5,
            },
        ]);
        let low_level = generate_dummy_cache(tempfile::tempdir().unwrap(), 100_000);
        let mut high_level = TileCache::new(low_level).unwrap();
        let mut receiver = high_level.get_receiver().unwrap();

        // Register, drop and register again before the answers arrive.
        high_level.register_new_interest_area(0, test_rect);
        high_level.completely_unsubscribe(0);
        high_level.register_new_interest_area(0, test_rect);

        let message = receiver.recv().await.unwrap();
        assert_matches!(message, CachingResultMessage::InitializationCompleted);
        high_level.process_caching_message(message);

        let mut counter = 0;
        while let Some(message) = receiver.recv().await {
            counter += 1;
            assert_matches!(message, CachingResultMessage::TileData { level: 5, .. });
            high_level.process_caching_message(message);
            if counter == TILE_DIMENSION * TILE_DIMENSION {
                break;
            }
        }
        assert_eq!(high_level.number_of_tiles_failed(), 0);

        let client_images = high_level.get_all_images_for_client(0);
        assert_eq!(client_images.len(), TILE_DIMENSION as usize * TILE_DIMENSION as usize);
        assert_eq!(client_images[0].position.zoom, 5);
   
        high_level.completely_unsubscribe(0);
        let client_images = high_level.get_all_images_for_client(0);
        assert!(client_images.is_empty());
    }
}
