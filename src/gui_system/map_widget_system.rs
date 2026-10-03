//! This module contains a structure that administrates all the different map widgets and
//! the internal cache.

use crate::annotation_system::waypoint_system::{WaypointKey, WaypointSystem};
use crate::gui_system::high_level_tile_cache::{CacheUpdateMessage, TileCache};
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::{
    AnnotationOverlay, FocalPoint, MapInteractionCommand, MapWidget, SpecificInteractionCommand,
};
use crate::tile_cache::cache_core::CachingResultMessage;
use iced::widget::{canvas, stack};
use iced::{Element, Fill, Rectangle, Task};
use tokio_stream::wrappers::ReceiverStream;

/// The messages dealing with the widgets these are messages from the
/// caching system and messages dealing with map interaction.
#[derive(Debug, Clone)]
pub enum MapWidgetMessage {
    /// These are internal messages coming from the caching system basically flagging the arrival of data.
    CachingResultMessage(CachingResultMessage),
    /// The messages and events from the real map interaction.
    MapInteractionCommand(MapInteractionCommand),
}

impl From<MapInteractionCommand> for MapWidgetMessage {
    fn from(command: MapInteractionCommand) -> Self {
        MapWidgetMessage::MapInteractionCommand(command)
    }
}

impl From<CachingResultMessage> for MapWidgetMessage {
    fn from(command: CachingResultMessage) -> Self {
        MapWidgetMessage::CachingResultMessage(command)
    }
}

/// What the map system produced while processing a message. This is the public
/// output of [`MapWidgetSystem::process_message`].
///
/// These are one shot notifications, the system keeps no state of its own about
/// them. An application that wants a selection to persist, and to be resettable,
/// holds that state itself.
#[derive(Debug, Clone)]
pub enum MapEvent {
    /// Something went wrong, with a text meant for the user.
    Error(String),
    /// The user picked this way point. `client_id` says in which widget that
    /// happened, it does not make that widget an owner of anything.
    WaypointSelected { client_id: u32, key: WaypointKey },
    /// A soft focus started with [`MapWidgetSystem::animate_to`] has arrived. Not
    /// sent for an animation that was cancelled or replaced on the way.
    FocusReached { client_id: u32 },
    /// The user clicked the direction arrow of this flagged way point in the widget
    /// `client_id`. What follows is up to the application, typically a focus of that
    /// widget on the way point.
    ArrowClicked { client_id: u32, key: WaypointKey },
    /// On the client simply a map position with latitude. longitude has been selected.
    /// If there is some kind of annotation on the map this information would dominate.
    MapPositionClicked {
        client_id: u32,
        position: LatitudeLongitude,
    },
}

/// The map widget system administrates all the widgets in combination with a
/// tile map cache.
pub struct MapWidgetSystem {
    tile_cache: TileCache,
    widget_collection: Vec<MapWidget>,
    // TODO: Here we will also insert the landmark and track system
    waypoint_system: WaypointSystem,
}

impl MapWidgetSystem {
    /// Generates our instance and the stream for the messages.
    /// The Tile cache is opaque but gets contructed in [`super::tile_cache_construction::TileCacheConfig`]
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
                waypoint_system: Default::default(),
            },
            task,
        )
    }

    /// Read access to the way point system.
    pub fn get_waypoints(&self) -> &WaypointSystem {
        &self.waypoint_system
    }

    /// Gets a mutable access for the way point system to modify things.
    pub fn get_waypoint_system_as_mut(&mut self) -> &mut WaypointSystem {
        for widget in &mut self.widget_collection {
            widget.request_focal_reset();
        }
        &mut self.waypoint_system
    }

    ///  The messages going into the caching system are processed here.
    fn process_caching_message(&mut self, message: CachingResultMessage) -> Option<MapEvent> {
        let mut final_message = "".to_string();
        self.tile_cache.process_caching_message(message);
        for msg in self.tile_cache.drain_result_messages() {
            match msg {
                CacheUpdateMessage::ErrorMessage { text: msg } => {
                    final_message += " ";
                    final_message += &msg;
                }
                CacheUpdateMessage::RelevantTilesArrived { client } => {
                    let new_tiles = self.tile_cache.get_all_images_for_client(client);
                    self.widget_collection[client as usize].set_drawing_tiles(new_tiles);
                }
            }
        }
        (!final_message.is_empty()).then_some(MapEvent::Error(final_message))
    }

    /// Moves a widget to a new view and hands it what it needs to draw there.
    fn apply_focal_point(&mut self, client_id: u32, point: FocalPoint, rectangle: Rectangle) {
        let result = self.widget_collection[client_id as usize].apply_focal_point(point, rectangle);
        match result {
            Some(bounding) => {
                self.tile_cache
                    .register_new_interest_area(client_id, bounding);
                // We have to reset the tiles here, because they may already exist from one of the other clients.
                let tiles = self.tile_cache.get_all_images_for_client(client_id);
                self.widget_collection[client_id as usize].set_drawing_tiles(tiles);
                // TODO: Here we will add also the other information for the paths and regions.
                let way_points = self.waypoint_system.get_all_relevant_waypoints(&bounding);
                let flagged = self.waypoint_system.get_all_flagged_waypoints();
                self.widget_collection[client_id as usize].set_waypoint_info(way_points, flagged);
            }
            None => self.tile_cache.completely_unsubscribe(client_id),
        }
    }

    /// Processes the messsage and eventually returns a map event for further processing.
    fn process_widget_message(
        &mut self,
        client_id: u32,
        message: SpecificInteractionCommand,
    ) -> Option<MapEvent> {
        match message {
            SpecificInteractionCommand::SetFocalPoint(point, rectangle) => {
                self.apply_focal_point(client_id, point, rectangle);
                None
            }

            SpecificInteractionCommand::AnimationFrame {
                focal_point,
                bounds,
                generation,
                finished,
            } => {
                let widget = &mut self.widget_collection[client_id as usize];
                // A frame of an animation that was replaced or cancelled after it
                // had been published would drag the view back onto the old path.
                if !widget.is_current_animation(generation) {
                    return None;
                }
                if finished {
                    widget.cancel_animation();
                }
                // Setting the focal point initiates the redraw which itself generates an animation
                // frame command.
                self.apply_focal_point(client_id, focal_point, bounds);
                match finished {
                    true => Some(MapEvent::FocusReached { client_id }),
                    false => None,
                }
            }

            SpecificInteractionCommand::WaypointClicked(key) => {
                // The widget reports what the click hit, the meaning is decided here.
                // A way point that is gone by now must not reach the application, the
                // snapshot in the widget can be older than the collection.
                self.waypoint_system.get_waypoint_info(key).map(|_| MapEvent::WaypointSelected { client_id, key })
            }

            // In this case we have simply clicked somewhere on the map.
            SpecificInteractionCommand::MapPointClicked(position) => {
                Some(MapEvent::MapPositionClicked {
                    client_id,
                    position,
                })
            }

            SpecificInteractionCommand::ArrowClicked(key) => {
                // Same reasoning as for the way point, and a way point that lost its
                // flag in the meantime has no arrow any more either.
                match self.waypoint_system.get_waypoint_info(key) {
                    Some(point) if point.flag.is_some() => {
                        Some(MapEvent::ArrowClicked { client_id, key })
                    }
                    _ => None,
                }
            }
        }
    }

    /// Processes all the relevant messages and reports what came out of it.
    pub fn process_message(&mut self, message: MapWidgetMessage) -> Option<MapEvent> {
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
            self.tile_cache.get_copyright_text(),
            FocalPoint {
                position: LatitudeLongitude::new(49.75, 6.63),
                continuous_zoom_level: 12.0,
            },
        ));
        id
    }

    /// The view the widget currently shows.
    pub fn focal_point(&self, id: u32) -> FocalPoint {
        self.widget_collection[id as usize].focal_point()
    }

    /// The hard focus: the widget jumps to `focal_point` without any animation. A
    /// soft focus still running on that widget is cancelled.
    pub fn set_focal_point(&mut self, id: u32, focal_point: FocalPoint) {
        self.widget_collection[id as usize].set_focal_point(focal_point);
    }

    /// The soft focus: the widget animates to `target` and ends at the zoom level it
    /// started with. Close targets are reached by a pan, far ones by zooming out,
    /// panning and zooming back in. User panning and zooming is suspended meanwhile,
    /// and [`MapEvent::FocusReached`] reports the arrival.
    pub fn animate_to(&mut self, id: u32, target: LatitudeLongitude) {
        self.widget_collection[id as usize].animate_to(target);
    }

    /// The canvas stack for one widget. Returns an `Element`, because the map tiles
    /// and the annotations have to sit in two stacked canvases to end up in separate
    /// render layers, see `AnnotationOverlay`.
    pub fn canvas(&self, id: u32) -> Element<'_, MapInteractionCommand> {
        let widget = self
            .widget_collection
            .get(id as usize)
            .expect("unknown widget id");

        stack![
            canvas(widget).width(Fill).height(Fill),
            canvas(AnnotationOverlay::new(widget))
                .width(Fill)
                .height(Fill),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Retries to load the failed tiles.
    pub fn retry_failed_tiles(&mut self) {
        self.tile_cache.retry_failed_tiles();
    }

    /// Checks the number of failed tiles.
    pub fn number_of_tiles_failed(&self) -> u32 {
        self.tile_cache.number_of_tiles_failed()
    }
}
