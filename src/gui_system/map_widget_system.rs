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
    /// The user clicked the direction arrow of this flagged annotation in the widget
    /// `client_id`. What follows is up to the application, typically a focus of that
    /// widget on the annotation.
    ArrowClicked { client_id: u32, key: AnnotationKey },
    /// The user clicked a spot of the map in the widget `client_id` where there is no
    /// annotation and no direction arrow; those give their own events instead.
    MapPositionClicked {
        client_id: u32,
        position: LatitudeLongitude,
    },
    /// The files of the tile cache are deleted, see [`MapWidgetSystem::clear_cache`].
    /// A problem on the way is reported as [`MapEvent::Error`] instead.
    CacheCleared,
}

/// What [`MapWidgetSystem::clear_cache`] does besides deleting the cached files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheClearing {
    /// The tiles on screen are dropped and downloaded again, and the cache fills up
    /// anew. Meant for broken tiles the server delivered, or for tiles that are
    /// outdated after long use.
    Refresh,
    /// The tiles on screen stay as they are, and nothing gets written to disc any
    /// more for the rest of the session; tiles are still downloaded and shown. Meant
    /// for removing the cache before the program is no longer used: purge, wait for
    /// [`MapEvent::CacheCleared`] and exit.
    Purge,
}

/// The map widget system administrates all the widgets in combination with a
/// tile map cache.
///
/// # Example
/// A minimal application with one map widget. The task of [`Self::boot`] and the
/// commands of the canvas both come back as [`MapWidgetMessage`]s into
/// [`Self::process_message`], which turns them into [`MapEvent`]s.
/// ```no_run
/// use iced::{Element, Task};
/// use map_iced::gui_system::map_widget_system::{MapEvent, MapWidgetMessage, MapWidgetSystem};
/// use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
///
/// struct Application {
///     map: MapWidgetSystem,
///     widget_id: u32,
/// }
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     Map(MapWidgetMessage),
/// }
///
/// impl Application {
///     fn boot() -> (Self, Task<Message>) {
///         // Your own application name and e-mail instead of the placeholder.
///         let config = TileCacheConfig::open_street_map("PlaceholderMapping");
///         let (mut map, task) = MapWidgetSystem::boot(config).expect("tile cache set up");
///         let widget_id = map.request_new_widget();
///         (Self { map, widget_id }, task.map(Message::Map))
///     }
///
///     fn update(&mut self, message: Message) {
///         let Message::Map(message) = message;
///         if let Some(MapEvent::Error(text)) = self.map.process_message(message) {
///             eprintln!("{text}");
///         }
///     }
///
///     fn view(&self) -> Element<'_, Message> {
///         self.map
///             .canvas(self.widget_id)
///             .map(|command| Message::Map(command.into()))
///     }
/// }
///
/// fn main() -> iced::Result {
///     iced::application(Application::boot, Application::update, Application::view).run()
/// }
/// ```
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
    ///
    /// # Example
    /// A configuration read from JSON, with the test pattern as fallback if it is
    /// broken. The task has to be returned from the application's boot function.
    /// `PlaceholderMapping` stands for your own application name and e-mail.
    /// ```no_run
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// let json = r#"{
    ///     "cache": { "CacheDirectory": "OpenStreetMap" },
    ///     "cache_size": 100000000,
    ///     "source": { "OpenStreetMap": { "user_agent": "PlaceholderMapping" } }
    /// }"#;
    /// let (system, task) = TileCacheConfig::from_json_str(json)
    ///     .and_then(MapWidgetSystem::boot)
    ///     .unwrap_or_else(|e| {
    ///         eprintln!("{e}, showing the test pattern instead");
    ///         MapWidgetSystem::boot(TileCacheConfig::test_pattern()).expect("test pattern set up")
    ///     });
    /// ```
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
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::DistanceUnits;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// assert_eq!(system.distance_units(), DistanceUnits::Metric);
    /// // The same units in the application's own texts as on the scale bars.
    /// let text = system.distance_units().distance_text(1_240.0);
    /// assert_eq!(text, "1.24 km");
    /// ```
    pub fn distance_units(&self) -> DistanceUnits {
        self.distance_units
    }

    /// Switches the scale bars of all widgets, present and future, to `distance_units`.
    /// Descriptions are the application's texts and stay as they are.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::DistanceUnits;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// system.set_distance_units(DistanceUnits::Imperial);
    /// assert_eq!(system.distance_units(), DistanceUnits::Imperial);
    /// ```
    pub fn set_distance_units(&mut self, distance_units: DistanceUnits) {
        self.distance_units = distance_units;
        for widget in &mut self.widget_collection {
            widget.set_distance_units(distance_units);
        }
    }

    /// Requires a reset for all widgets, because some annotation information may have changed.
    fn reset_all_widgets(&mut self) {
        for widget in &mut self.widget_collection {
            widget.request_focal_reset();
        }
    }

    /// Mutable access to the way point system, to add, change and delete way points.
    /// Every widget is reset with it, so that the changes show; for queries use
    /// [`Self::waypoints`], which has no such cost.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::WaypointSymbol;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let key = system.waypoints_mut().add_way_point(
    ///     WaypointSymbol::cross(Color::from_rgb(1.0, 0.0, 0.0)),
    ///     LatitudeLongitude::new(49.7499, 6.6371),
    ///     Some("Porta Nigra".to_string()),
    /// );
    /// ```
    pub fn waypoints_mut(&mut self) -> &mut WaypointSystem {
        self.reset_all_widgets();
        self.annotation_system.waypoint_system()
    }

    /// Mutable access to the region system, to add, change and delete regions. Every
    /// widget is reset with it, so that the changes show; for queries use
    /// [`Self::regions`], which has no such cost.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::region_system::{RegionShape, RegionStyle};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let style = RegionStyle { fill: Color::from_rgba(1.0, 0.0, 0.0, 0.3), border: Color::from_rgb(1.0, 0.0, 0.0) };
    /// let circle = RegionShape::Circle { center: LatitudeLongitude::new(49.75, 6.63), radius_meter: 2_000.0 };
    /// let key = system.regions_mut().add_region(circle, style, None);
    /// assert!(key.is_ok());
    /// ```
    pub fn regions_mut(&mut self) -> &mut RegionSystem {
        self.reset_all_widgets();
        self.annotation_system.region_system()
    }

    /// Mutable access to the path system, to add, change and delete paths. Every
    /// widget is reset with it, so that the changes show; for queries use
    /// [`Self::paths`], which has no such cost.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.paths_mut().add_path(track, Color::BLACK, None);
    /// assert!(key.is_ok());
    /// ```
    pub fn paths_mut(&mut self) -> &mut PathSystem {
        self.reset_all_widgets();
        self.annotation_system.path_system()
    }

    /// Sets the description of the annotation `key`, whatever its kind. Unlike the
    /// update methods behind [`Self::waypoints_mut`] and its siblings,
    /// this does not reset the widgets: they only swap their copy of the one
    /// annotation and keep everything drawn. That makes it cheap enough to answer
    /// every [`MapEvent::AnnotationHovered`], e.g. with the distance along a track.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::annotation_support::{AnnotationError, AnnotationKey};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let track = vec![LatitudeLongitude::new(50.0, 7.0), LatitudeLongitude::new(50.0, 7.1)];
    /// let path_key = system.paths_mut().add_path(track, Color::BLACK, None).expect("two positions");
    /// let key = AnnotationKey::from(path_key);
    ///
    /// // As the answer to a MapEvent::AnnotationHovered with this hover point.
    /// let hover_point = LatitudeLongitude::new(50.0, 7.05);
    /// let location = system.paths().closest_point_on_path(path_key, hover_point).expect("present");
    /// let text = system.distance_units().distance_text(location.distance_from_start_meter);
    /// assert!(system.update_description(key, Some(format!("{text} from the start"))).is_ok());
    ///
    /// system.paths_mut().delete_path(path_key).expect("present");
    /// assert_eq!(system.update_description(key, None), Err(AnnotationError::KeyNotContained));
    /// ```
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

    /// Read-only access to the region system, to query things without the widget
    /// reset that comes with [`Self::regions_mut`].
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::region_system::{RegionShape, RegionStyle};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let style = RegionStyle { fill: Color::WHITE, border: Color::BLACK };
    /// let circle = RegionShape::Circle { center: LatitudeLongitude::new(49.75, 6.63), radius_meter: 2_000.0 };
    /// let key = system.regions_mut().add_region(circle, style, None).expect("a valid circle");
    /// assert!(matches!(system.regions().shape(key), Ok(RegionShape::Circle { .. })));
    /// ```
    pub fn regions(&self) -> &RegionSystem {
        &self.annotation_system.region_system
    }

    /// Read-only access to the path system, to query things without the widget
    /// reset that comes with [`Self::paths_mut`].
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.paths_mut().add_path(track.clone(), Color::BLACK, None).expect("two positions");
    /// assert_eq!(system.paths().positions(key), Ok(track.as_slice()));
    /// ```
    pub fn paths(&self) -> &PathSystem {
        &self.annotation_system.path_system
    }

    /// Read-only access to the way point system, to query things without the widget
    /// reset that comes with [`Self::waypoints_mut`], e.g. the way point behind a
    /// [`MapEvent::AnnotationSelected`].
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::waypoint_system::WaypointSymbol;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let porta_nigra = LatitudeLongitude::new(49.7499, 6.6371);
    /// let key = system.waypoints_mut().add_way_point(WaypointSymbol::cross(Color::WHITE), porta_nigra, None);
    /// assert_eq!(system.waypoints().position(key), Ok(porta_nigra));
    /// ```
    pub fn waypoints(&self) -> &WaypointSystem {
        &self.annotation_system.waypoint_system
    }

    /// Renamed to [`Self::waypoints_mut`].
    #[deprecated(since = "0.2.0", note = "renamed to `waypoints_mut`")]
    pub fn get_waypoint_system_as_mut(&mut self) -> &mut WaypointSystem {
        self.waypoints_mut()
    }

    /// Renamed to [`Self::regions_mut`].
    #[deprecated(since = "0.2.0", note = "renamed to `regions_mut`")]
    pub fn get_region_system_as_mut(&mut self) -> &mut RegionSystem {
        self.regions_mut()
    }

    /// Renamed to [`Self::paths_mut`].
    #[deprecated(since = "0.2.0", note = "renamed to `paths_mut`")]
    pub fn get_path_system_as_mut(&mut self) -> &mut PathSystem {
        self.paths_mut()
    }

    /// Renamed to [`Self::regions`].
    #[deprecated(since = "0.2.0", note = "renamed to `regions`")]
    pub fn get_region_system(&self) -> &RegionSystem {
        self.regions()
    }

    /// Renamed to [`Self::paths`].
    #[deprecated(since = "0.2.0", note = "renamed to `paths`")]
    pub fn get_path_system(&self) -> &PathSystem {
        self.paths()
    }

    ///  The messages going into the caching system are processed here.
    fn process_caching_message(&mut self, message: CachingResultMessage) -> Option<MapEvent> {
        let mut final_message = "".to_string();
        let mut cache_cleared = false;
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
                CacheUpdateMessage::CacheCleared => cache_cleared = true,
            }
        }
        match (final_message.is_empty(), cache_cleared) {
            (false, _) => Some(MapEvent::Error(final_message)),
            (true, true) => Some(MapEvent::CacheCleared),
            (true, false) => None,
        }
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

    /// Processes the message and possibly returns a map event for further processing.
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
                // An annotation that is gone by now must not reach the application, the
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
                // Same reasoning as for the click, and an annotation that lost its
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
    ///
    /// # Example
    /// The update function of an application: the system first, then the
    /// application's answer to what came out of it.
    /// ```
    /// use map_iced::gui_system::map_widget_system::{MapEvent, MapWidgetMessage, MapWidgetSystem};
    /// fn update(system: &mut MapWidgetSystem, message: MapWidgetMessage) {
    ///     match system.process_message(message) {
    ///         // Clicked annotations and arrows: the widget heads there.
    ///         Some(MapEvent::AnnotationSelected { client_id, key, .. })
    ///         | Some(MapEvent::ArrowClicked { client_id, key }) => {
    ///             if let Ok(target) = system.focus_target(key, None) {
    ///                 system.animate_to(client_id, target);
    ///             }
    ///         }
    ///         Some(MapEvent::Error(text)) => eprintln!("{text}"),
    ///         _ => {}
    ///     }
    /// }
    /// ```
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
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// // An overview and a detail view, each with a view of its own.
    /// let overview = system.request_new_widget();
    /// let detail = system.request_new_widget();
    /// assert_ne!(overview, detail);
    /// system.jump_to(detail, LatitudeLongitude::new(49.7499, 6.6371));
    /// ```
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
    /// cancelled. Right after [`Self::request_new_widget`] the widget has no size yet;
    /// an area is then fitted with the first layout, so a start view can be set up
    /// this way.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let widget = system.request_new_widget();
    /// // A position converts into a FocusTarget, the zoom level stays.
    /// system.jump_to(widget, LatitudeLongitude::new(49.7499, 6.6371));
    /// ```
    pub fn jump_to(&mut self, id: u32, target: impl Into<FocusTarget>) {
        self.widget_collection[id as usize].jump_to(&target.into());
    }

    /// The soft focus: the widget animates to `target`. A position ends at the zoom
    /// level the widget started with, an area at the level that fits it. Close
    /// targets are reached by a pan, far ones by zooming out, panning and zooming in.
    /// User panning and zooming is suspended meanwhile, and
    /// [`MapEvent::FocusReached`] reports the arrival. A widget that has not been laid
    /// out yet has no view to animate from, it does a [`Self::jump_to`] instead.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let widget = system.request_new_widget();
    /// // Far away from the start view: zooms out, pans and zooms in again.
    /// system.animate_to(widget, LatitudeLongitude::new(48.8566, 2.3522));
    /// ```
    pub fn animate_to(&mut self, id: u32, target: impl Into<FocusTarget>) {
        self.widget_collection[id as usize].animate_to(&target.into());
    }

    /// The focus target for the annotation `key`, to hand to [`Self::jump_to`] or
    /// [`Self::animate_to`]. An annotation with an extent, like a region, is fitted to
    /// cover the share `fill` of the widget's width or height, [`Self::DEFAULT_FOCUS_FILL`]
    /// if `None`; a way point becomes its position and `fill` is ignored.
    /// [`AnnotationError::KeyNotContained`] if there is no such annotation.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::region_system::{RegionShape, RegionStyle};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::{FocusTarget, MapWidgetSystem};
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let style = RegionStyle { fill: Color::WHITE, border: Color::BLACK };
    /// let circle = RegionShape::Circle { center: LatitudeLongitude::new(49.75, 6.63), radius_meter: 2_000.0 };
    /// let key = system.regions_mut().add_region(circle, style, None).expect("a valid circle");
    /// let widget = system.request_new_widget();
    ///
    /// // The circle fills half the widget, with plenty of surroundings.
    /// let target = system.focus_target(key.into(), Some(0.5)).expect("present");
    /// assert!(matches!(target, FocusTarget::Area { .. }));
    /// system.animate_to(widget, target);
    /// ```
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
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::region_system::{RegionShape, RegionStyle};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let style = RegionStyle { fill: Color::WHITE, border: Color::BLACK };
    /// let apex = LatitudeLongitude::new(49.75, 6.63);
    /// let sector = RegionShape::Sector { apex, radius_meter: 5_000.0, heading_degree: 90.0, opening_degree: 60.0 };
    /// let key = system.regions_mut().add_region(sector, style, None).expect("a valid sector");
    /// // A sector stands for its apex, the source of the plume.
    /// assert_eq!(system.center(key.into()), Ok(apex));
    /// ```
    pub fn center(&self, key: AnnotationKey) -> Result<LatitudeLongitude, AnnotationError> {
        self.annotation_system
            .get_specific_element(key)
            .map(|annotation| annotation.get_center())
    }

    /// The canvas stack for one widget. Returns an `Element`, because the map tiles
    /// and the annotations have to sit in two stacked canvases to end up in separate
    /// render layers, see `AnnotationOverlay`.
    ///
    /// # Example
    /// The view of an application with two map widgets side by side.
    /// ```
    /// use iced::Element;
    /// use iced::widget::row;
    /// use map_iced::gui_system::map_widget_system::{MapWidgetMessage, MapWidgetSystem};
    /// #[derive(Debug, Clone)]
    /// enum Message {
    ///     Map(MapWidgetMessage),
    /// }
    ///
    /// fn view(system: &MapWidgetSystem, widget_ids: [u32; 2]) -> Element<'_, Message> {
    ///     row(widget_ids.map(|id| system.canvas(id).map(|command| Message::Map(command.into()))))
    ///         .spacing(10)
    ///         .into()
    /// }
    /// ```
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
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// // E.g. behind a button that is only enabled while tiles are missing.
    /// if system.number_of_tiles_failed() > 0 {
    ///     system.retry_failed_tiles();
    /// }
    /// ```
    pub fn retry_failed_tiles(&mut self) {
        self.tile_cache.retry_failed_tiles();
    }

    /// Deletes all files of the local tile cache, below the cache directory of the
    /// configuration; files the cache has not written stay. `mode` decides what
    /// happens with the tiles on screen, see [`CacheClearing`]. The deletion runs in
    /// the background, [`MapEvent::CacheCleared`] reports its end.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::map_widget_system::{CacheClearing, MapWidgetSystem};
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// // E.g. behind a button, when the server delivered broken tiles.
    /// system.clear_cache(CacheClearing::Refresh);
    /// ```
    pub fn clear_cache(&mut self, mode: CacheClearing) {
        self.tile_cache
            .clear_cache(matches!(mode, CacheClearing::Refresh));
    }

    /// Checks the number of failed tiles.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// // Nothing has been requested yet, so nothing has failed.
    /// assert_eq!(system.number_of_tiles_failed(), 0);
    /// ```
    pub fn number_of_tiles_failed(&self) -> u32 {
        self.tile_cache.number_of_tiles_failed()
    }
}
