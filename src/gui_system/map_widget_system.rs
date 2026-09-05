//! This module contains a structure that administrates all the different map widgets and
//! the internal cache.

use iced::Task;
use tokio_stream::wrappers::ReceiverStream;
use crate::gui_system::high_level_tile_cache::{CacheUpdateMessage, TileCache};
use crate::gui_system::map_widget::{MapInteractionCommand, MapWidget};
use crate::gui_system::math_coordinates::{
    BoundingRectangle, RectConversionError, RequestRectangle,
};
use crate::tile_cache::cache_core::CachingResultMessage;

#[derive(Debug, Clone)]
pub enum MapWidgetMessage {
    CachingResultMessage(CachingResultMessage),
    TileRequesterMessage {
        request_rectangle: RequestRectangle,
        client_id: u32,
    },
    MapInteractionCommand(MapInteractionCommand),
}

impl From<MapInteractionCommand> for MapWidgetMessage {
    fn from(command: MapInteractionCommand) -> Self {
        MapWidgetMessage::MapInteractionCommand(command)
    }
}

pub struct MapWidgetSystem {
    tile_cache: TileCache,
    widget_collection: Vec<MapWidget>,
}

impl MapWidgetSystem {
    /// Generates our instance and the stream for the messages.
    pub fn boot(mut tile_cache: TileCache) -> (Self, Task<MapWidgetMessage>) {
        let receiver = tile_cache.get_receiver().expect("fresh cache has a receiver");
        let task = Task::run(ReceiverStream::new(receiver), MapWidgetMessage::CachingResultMessage);
        (Self { tile_cache, widget_collection: Vec::new() }, task)
    }

    ///  The messages going into the caching system are processed here.
    fn process_caching_message(&mut self, message: CachingResultMessage) {
        self.tile_cache.process_caching_message(message.clone());
        for msg in self.tile_cache.drain_result_messages() {
            match msg {
                CacheUpdateMessage::ErrorMessage { text } => {} // TODO: Error display has be be added later.
                CacheUpdateMessage::RelevantTilesArrived { client } => {
                    let new_tiles = self.tile_cache.get_all_images_for_client(client);
                    self.widget_collection[client as usize].set_drawing_tiles(new_tiles);
                }
            }
        }
    }

    /// Processes the request for a new rectangle.
    fn process_request(&mut self, request_rectangle: RequestRectangle, client_id: u32) {
        match BoundingRectangle::try_from(&request_rectangle) {
            Ok(plain_rect) => {
                self.tile_cache
                    .register_new_interest_area(client_id, plain_rect);
            }
            Err(RectConversionError::NegativeSize) => {
                debug_assert!(false, "Negative size on incoming rect");
            }
            Err(RectConversionError::OutOfWorld) => {} // This is ok nothing to do here.
        }
    }

    /// Processes all the relevant messages.
    pub fn process_message(&mut self, message: MapWidgetMessage) {
        match message {
            MapWidgetMessage::CachingResultMessage(msg) => self.process_caching_message(msg),
            MapWidgetMessage::TileRequesterMessage {
                request_rectangle,
                client_id,
            } => self.process_request(request_rectangle, client_id),
            MapWidgetMessage::MapInteractionCommand(MapInteractionCommand{client_id, command}) => {todo!("Command processing missing.")} // TODO: Pass on the interaction command to the client.
        }
    }

    /// Requests a new widget and returns the handle for it.
    pub fn request_new_widget(&mut self) -> u32 {
        let id = self.widget_collection.len() as u32;
        self.widget_collection.push(MapWidget::new(id));
        id
    }

    /// Gets a read only version of the indicated widget.
    pub fn get_widget_access(&self, id: u32) -> Option<&MapWidget> {
        self.widget_collection.get(id as usize)
    }
}
