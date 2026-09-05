//! This module contains a structure that administrates all the different map widgets and
//! the internal cache.

use crate::gui_system::high_level_tile_cache::{CacheUpdateMessage, TileCache};
use crate::gui_system::map_widget::{
    FocalPoint, MapInteractionCommand, MapWidget, SpecificInteractionCommand,
};
use crate::gui_system::math_coordinates::LatitudeLongitude;
use crate::tile_cache::cache_core::CachingResultMessage;
use iced::Task;
use iced::widget::{Canvas, canvas};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Debug, Clone)]
pub enum MapWidgetMessage {
    CachingResultMessage(CachingResultMessage),
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
        let receiver = tile_cache
            .get_receiver()
            .expect("fresh cache has a receiver");
        let task = Task::run(
            ReceiverStream::new(receiver),
            MapWidgetMessage::CachingResultMessage,
        );
        (
            Self {
                tile_cache,
                widget_collection: Vec::new(),
            },
            task,
        )
    }

    ///  The messages going into the caching system are processed here.
    fn process_caching_message(&mut self, message: CachingResultMessage) {
        self.tile_cache.process_caching_message(message);
        for msg in self.tile_cache.drain_result_messages() {
            match msg {
                CacheUpdateMessage::ErrorMessage { text: _ } => {
                    todo!("Implement error case.")
                } // TODO: Error display has be be added later.
                CacheUpdateMessage::RelevantTilesArrived { client } => {
                    let new_tiles = self.tile_cache.get_all_images_for_client(client);
                    self.widget_collection[client as usize].set_drawing_tiles(new_tiles);
                }
            }
        }
    }

    fn process_widget_message(&mut self, client_id: u32, message: SpecificInteractionCommand) {
        match message {
            SpecificInteractionCommand::SetFocalPoint(point, rectangle) => {
                let result =
                    self.widget_collection[client_id as usize].apply_focal_point(point, rectangle);
                if let Some(bounding) = result {
                    self.tile_cache
                        .register_new_interest_area(client_id, bounding);
                } else {
                    // Nothing for our client.
                    self.tile_cache.completely_unsubscribe(client_id);
                }
            }
        }
    }

    /// Processes all the relevant messages.
    pub fn process_message(&mut self, message: MapWidgetMessage) {
        match message {
            MapWidgetMessage::CachingResultMessage(msg) => self.process_caching_message(msg),
            MapWidgetMessage::MapInteractionCommand(MapInteractionCommand {
                client_id,
                command,
            }) => self.process_widget_message(client_id, command),
        }
    }

    /// Requests a new widget and returns the handle for it.
    pub fn request_new_widget(&mut self) -> u32 {
        let id = self.widget_collection.len() as u32;
        self.widget_collection.push(MapWidget::new(
            id,
            FocalPoint {
                position: LatitudeLongitude::new(49.75, 6.63),
                continuous_zoom_level: 12.0,
            },
        ));
        id
    }

    /// The canvas for one widget. Returns `Canvas`, not `Element`, so the
    /// caller keeps full control over layout.
    pub fn canvas(&self, id: u32) -> Canvas<&MapWidget, MapInteractionCommand> {
        canvas(
            self.widget_collection
                .get(id as usize)
                .expect("unknown widget id"),
        )
    }
}
