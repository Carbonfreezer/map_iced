//! This module contains a structure that administrates all the different map widgets and
//! the internal cache.

use crate::annotation_system::annotation_compound::AnnotationSystem;
use crate::annotation_system::annotation_support::{AnnotationError, AnnotationKey};
use crate::annotation_system::path_system::PathSystem;
use crate::annotation_system::region_system::RegionSystem;
use crate::annotation_system::waypoint_system::WaypointSystem;
use crate::gui_system::distance_units::DistanceUnits;
use crate::gui_system::hashmap_stable::HashmapStable;
use crate::gui_system::high_level_tile_cache::{CacheUpdateMessage, TileCache};
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::map_widget_components::{
    AnnotationOverlay, FocalPoint, SpecificInteractionCommand,
};
use crate::gui_system::map_widget::map_widget_core::MapWidget;
use crate::gui_system::tile_cache_construction::{TileCacheConfig, TileCacheError};
use crate::tile_cache::cache_core::CachingResultMessage;
use iced::widget::{canvas, stack};
use iced::{Element, Fill, Rectangle, Task};
use tokio_stream::wrappers::ReceiverStream;

pub use crate::gui_system::map_widget::map_widget_components::{
    FocusTarget, MapInteractionCommand,
};

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
    /// The user picked this annotation element. `client_id` says in which widget that
    /// happened, it does not make that widget an owner of anything. The click point is also added to be eventually
    /// interpreted by the specific annotation component
    AnnotationSelected {
        client_id: u32,
        key: AnnotationKey,
        click_point: LatitudeLongitude,
    },
    /// The cursor has come onto this annotation element in the widget `client_id`,
    /// or moved on it where the label follows the cursor, as on a region or a path.
    /// The description shows up after a short dwell time, so there is time to adapt
    /// it to `hover_point` with [`MapWidgetSystem::update_description`] first.
    AnnotationHovered {
        client_id: u32,
        key: AnnotationKey,
        hover_point: LatitudeLongitude,
    },
    /// A soft focus started with [`MapWidgetSystem::animate_to`] has arrived. Not
    /// sent for an animation that was cancelled or replaced on the way.
    FocusReached { client_id: u32 },
    /// The user clicked the direction arrow of this flagged way point in the widget
    /// `client_id`. What follows is up to the application, typically a focus of that
    /// widget on the way point.
    ArrowClicked { client_id: u32, key: AnnotationKey },
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
    annotation_system: AnnotationSystem,
    distance_units: DistanceUnits,
}

impl MapWidgetSystem {
    /// The share of the widget a focused annotation covers when [`Self::focus_target`]
    /// gets no `fill`. Below 1.0, so the outline of a region keeps some distance
    /// from the widget edge instead of being clipped by it.
    pub const DEFAULT_FOCUS_FILL: f32 = 0.9;

    /// Sets up the tile cache described by `config` and returns the system together
    /// with the task that feeds the cache results back as [`MapWidgetMessage`]s.
    /// The config comes from JSON with [`TileCacheConfig::from_json_str`] or is built
    /// in code, e.g. with [`TileCacheConfig::open_street_map`].
    pub fn boot(config: TileCacheConfig) -> Result<(Self, Task<MapWidgetMessage>), TileCacheError> {
        let mut tile_cache = config.build()?;
        let receiver = tile_cache
            .get_receiver()
            .expect("fresh cache has a receiver");
        let task = Task::run(
            ReceiverStream::new(receiver),
            MapWidgetMessage::CachingResultMessage,
        );
        Ok((
            Self {
                tile_cache,
                widget_collection: Vec::new(),
                annotation_system: Default::default(),
                distance_units: Default::default(),
            },
            task,
        ))
    }

    /// The unit system the scale bars show, [`DistanceUnits::Metric`] unless set
    /// otherwise. Also meant for the application's own distance texts, e.g. with
    /// [`DistanceUnits::distance_text`] in an annotation description.
    pub fn distance_units(&self) -> DistanceUnits {
        self.distance_units
    }

    /// Switches the scale bars of all widgets, present and future, to `distance_units`.
    /// Descriptions are the application's texts and stay as they are.
    pub fn set_distance_units(&mut self, distance_units: DistanceUnits) {
        self.distance_units = distance_units;
        for widget in &mut self.widget_collection {
            widget.set_distance_units(distance_units);
        }
    }

    /// Requires a reset for all widgets, because some annotation information may have changed,
    fn reset_all_widgets(&mut self) {
        for widget in &mut self.widget_collection {
            widget.request_focal_reset();
        }
    }

    /// Gets a mutable access for the way point system to modify things.
    pub fn get_waypoint_system_as_mut(&mut self) -> &mut WaypointSystem {
        self.reset_all_widgets();
        self.annotation_system.waypoint_system()
    }

    /// Gets a mutable access for the region system to modify things.
    pub fn get_region_system_as_mut(&mut self) -> &mut RegionSystem {
        self.reset_all_widgets();
        self.annotation_system.region_system()
    }

    /// Gets a mutable access for the path system to modify things.
    pub fn get_path_system_as_mut(&mut self) -> &mut PathSystem {
        self.reset_all_widgets();
        self.annotation_system.path_system()
    }

    /// Sets the description of the annotation `key`, whatever its kind. Unlike the
    /// update methods behind [`Self::get_waypoint_system_as_mut`] and its siblings,
    /// this does not reset the widgets: they only swap their copy of the one
    /// annotation and keep everything drawn. That makes it cheap enough to answer
    /// every [`MapEvent::AnnotationHovered`], e.g. with the distance along a track.
    pub fn update_description(
        &mut self,
        key: AnnotationKey,
        description: Option<String>,
    ) -> Result<(), AnnotationError> {
        self.annotation_system
            .update_description(key, description)?;
        let annotation = self
            .annotation_system
            .get_specific_element(key)
            .expect("The description was just set");
        for widget in &mut self.widget_collection {
            widget.refresh_annotation(key, annotation.clone_box());
        }
        Ok(())
    }

    /// Gets a read-only access for the way point system, to query things without
    /// the widget reset that comes with [`Self::get_waypoint_system_as_mut`].
    pub fn get_waypoint_system(&self) -> &WaypointSystem {
        &self.annotation_system.waypoint_system
    }

    /// Gets a read-only access for the region system, to query things without the
    /// widget reset that comes with [`Self::get_region_system_as_mut`].
    pub fn get_region_system(&self) -> &RegionSystem {
        &self.annotation_system.region_system
    }

    /// Gets a read-only access for the path system, to query things without the
    /// widget reset that comes with [`Self::get_path_system_as_mut`].
    pub fn get_path_system(&self) -> &PathSystem {
        &self.annotation_system.path_system
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
        let Some(bounding) = result else {
            self.tile_cache.completely_unsubscribe(client_id);
            return;
        };
        self.tile_cache
            .register_new_interest_area(client_id, bounding);
        // We have to reset the tiles here, because they may already exist from one of the other clients.
        let tiles = self.tile_cache.get_all_images_for_client(client_id);
        self.widget_collection[client_id as usize].set_drawing_tiles(tiles);

        let converter = self.widget_collection[client_id as usize]
            .position_converter()
            .expect("Just applied focal point, position converted should exist.");
        let render_points = HashmapStable::new(
            self.annotation_system
                .get_complete_render_list()
                .filter(|(_, annotation)| converter.is_mercator_visible(annotation.cull_bounds()))
                .map(|(key, annotation)| (key, annotation.clone_box())),
        );
        let flagged_points = HashmapStable::new(
            self.annotation_system
                .get_complete_render_list()
                .filter(|(_, annotation)| annotation.get_flag().is_some())
                .map(|(key, annotation)| (key, annotation.clone_box())),
        );

        self.widget_collection[client_id as usize]
            .set_annotation_info(render_points, flagged_points);
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

            SpecificInteractionCommand::AnnotationClicked(key, click_point) => {
                // The widget reports what the click hit, the meaning is decided here.
                // A way point that is gone by now must not reach the application, the
                // snapshot in the widget can be older than the collection.
                self.annotation_system
                    .get_specific_element(key)
                    .map(|_| MapEvent::AnnotationSelected {
                        client_id,
                        key,
                        click_point,
                    })
                    .ok()
            }

            // Same reasoning as for the click.
            SpecificInteractionCommand::AnnotationHovered(key, hover_point) => self
                .annotation_system
                .get_specific_element(key)
                .map(|_| MapEvent::AnnotationHovered {
                    client_id,
                    key,
                    hover_point,
                })
                .ok(),

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
                match self.annotation_system.get_specific_element(key) {
                    Ok(annotation) if annotation.get_flag().is_some() => {
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
            self.distance_units,
        ));
        id
    }

    /// The hard focus on a [`FocusTarget`]: a position keeps the zoom level, an area
    /// is fitted into the widget. A soft focus still running on that widget is
    /// cancelled.
    pub fn jump_to(&mut self, id: u32, target: impl Into<FocusTarget>) {
        self.widget_collection[id as usize].jump_to(&target.into());
    }

    /// The soft focus: the widget animates to `target`. A position ends at the zoom
    /// level the widget started with, an area at the level that fits it. Close
    /// targets are reached by a pan, far ones by zooming out, panning and zooming in.
    /// User panning and zooming is suspended meanwhile, and
    /// [`MapEvent::FocusReached`] reports the arrival.
    pub fn animate_to(&mut self, id: u32, target: impl Into<FocusTarget>) {
        self.widget_collection[id as usize].animate_to(&target.into());
    }

    /// The focus target for the annotation `key`, to hand to [`Self::jump_to`] or
    /// [`Self::animate_to`]. An annotation with an extent, like a region, is fitted to
    /// cover the share `fill` of the widget's width or height, [`Self::DEFAULT_FOCUS_FILL`]
    /// if `None`; a way point becomes its position and `fill` is ignored.
    /// [`AnnotationError::KeyNotContained`] if there is no such annotation.
    pub fn focus_target(
        &self,
        key: AnnotationKey,
        fill: Option<f32>,
    ) -> Result<FocusTarget, AnnotationError> {
        let fill = fill.unwrap_or(Self::DEFAULT_FOCUS_FILL);
        self.annotation_system
            .get_specific_element(key)
            .map(|annotation| annotation.focus_target(fill))
    }

    /// The point the annotation `key` stands for, the one its direction arrow points
    /// at: the position of a way point, the centre of a circle, the apex of a sector,
    /// the centre of the bounds of a convex hull.
    /// [`AnnotationError::KeyNotContained`] if there is no such annotation.
    pub fn center(&self, key: AnnotationKey) -> Result<LatitudeLongitude, AnnotationError> {
        self.annotation_system
            .get_specific_element(key)
            .map(|annotation| annotation.get_center())
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
