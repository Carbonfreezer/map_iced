//! Way point example. The middle column lists the way points this application knows
//! about; the checkbox registers one with the map system or takes it out again, and
//! the row of a way point that was picked on a map is written in the accent colour.
//!
//! The focus button moves both maps to the way point, the left one with a hard jump
//! and the right one with the soft, animated focus. Mertert lies outside the start
//! view but close enough to be reached by a pan, Köln and Montréal need a zoom out.
//! A flagged way point gets a direction arrow while off screen; clicking it focuses
//! all maps, just like the focus button.

use iced::widget::text::Wrapping;
use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Alignment, Color, Element, Fill, FillPortion, Size, Task, Theme};
use map_iced::Bytes;
use map_iced::annotation_system::annotation_support::{AnnotationKey, DirectionFlag};
use map_iced::annotation_system::waypoint_system::WaypointSymbol;
use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
use map_iced::gui_system::map_widget::map_widget_components::FocusTarget;
use map_iced::gui_system::map_widget_system::{MapEvent, MapWidgetMessage, MapWidgetSystem};
use map_iced::gui_system::tile_cache_construction::{TileCacheConfig, tile_cache_debug_default};

/// The configuration data to load.
const CONFIG_DATA: &str = include_str!("../osm.json");

/// The icon we mark every way point with.
const ICON_DATA: &[u8] = include_bytes!("../assets/Icon.png");

/// The direction arrow of a flagged way point.
const FLAG: DirectionFlag = DirectionFlag {
    color: Color::from_rgb(0.95, 0.6, 0.1),
    priority: 0,
};

/// The direction arrow of the one urgent way point. Seen from Montréal, Köln lies in
/// the same direction as the points in Trier, and its arrow stays on top although
/// it is not the nearest.
const URGENT_FLAG: DirectionFlag = DirectionFlag {
    color: Color::from_rgb(0.9, 0.15, 0.15),
    priority: 1,
};

/// The way points this application offers, around the focal point the widgets start at,
/// with the flag each one gets. The last three lie further away, to try the focus with.
const CATALOGUE: [(&str, f64, f64, DirectionFlag); 9] = [
    ("Trier Dom", 49.7554, 6.6436, FLAG),
    ("Trier West", 49.7540, 6.6100, FLAG),
    ("Pallien", 49.7650, 6.6250, FLAG),
    ("Olewig", 49.7430, 6.6700, FLAG),
    ("Konz", 49.7020, 6.5800, FLAG),
    ("Mertert", 49.7031, 6.4797, FLAG),
    ("Köln", 50.9375, 6.9603, URGENT_FLAG),
    ("Montréal", 45.5017, -73.5673, FLAG),
    ("User defined", 49.754, 6.63, FLAG),
];

/// One row of the list. It carries the key for as long as the way point is
/// registered with the map system, which is also what the checkbox shows.
struct WaypointEntry {
    name: &'static str,
    position: LatitudeLongitude,
    key: Option<AnnotationKey>,
    /// The flag this way point gets when flagged.
    flag: DirectionFlag,
    /// Whether the way point is flagged, i.e. gets a direction arrow while off screen.
    flagged: bool,
}

struct WaypointApplication {
    widget_system: MapWidgetSystem,
    widget_ids: [u32; 2],
    entries: Vec<WaypointEntry>,
    /// The way point last picked on one of the maps. The map system reports a pick
    /// and then forgets about it, so holding on to it is our business.
    selected: Option<AnnotationKey>,
}

/// The overall message system in this application.
#[derive(Debug, Clone)]
enum Message {
    /// An internal message from the widget system.
    Widget(MapWidgetMessage),
    /// Register or unregister the way point of the given list row.
    Toggled(usize, bool),
    /// Move the maps to the way point of the given list row.
    Focus(usize),
    /// Flag or unflag the way point of the given list row.
    Flagged(usize, bool),
}

impl WaypointApplication {
    pub fn boot() -> (WaypointApplication, Task<Message>) {
        let cache = TileCacheConfig::from_json_str(CONFIG_DATA)
            .and_then(TileCacheConfig::build)
            .or_else(|e| {
                eprintln!("Configuration error: {e}");
                tile_cache_debug_default()
            })
            .expect("Should not be possible to reach");

        let (mut widget_system, task) = MapWidgetSystem::boot(cache);
        let widget_ids = [
            widget_system.request_new_widget(),
            widget_system.request_new_widget(),
        ];
        let entries = CATALOGUE
            .iter()
            .map(|(name, latitude, longitude, flag)| WaypointEntry {
                name,
                position: LatitudeLongitude::new(*latitude, *longitude),
                key: None,
                flag: *flag,
                flagged: false,
            })
            .collect();

        (
            Self {
                widget_system,
                widget_ids,
                entries,
                selected: None,
            },
            task.map(Message::Widget),
        )
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Widget(m) => {
                let Some(event) = self.widget_system.process_message(m) else {
                    return;
                };
                match event {
                    MapEvent::AnnotationSelected { key, .. } => self.selected = Some(key),
                    // The way point descriptions are fixed in this example.
                    MapEvent::FocusReached { .. } | MapEvent::AnnotationHovered { .. } => {}
                    // An arrow click acts like the focus button: all maps follow.
                    // The fill only matters for annotations with an extent.
                    MapEvent::ArrowClicked { key, .. } => {
                        if let Some(target) = self.widget_system.focus_target(key, None) {
                            self.focus_all(target);
                        }
                    }
                    // Error display is the subject of the other example, but
                    // swallowing them without a word would hide a broken setup.
                    MapEvent::Error(text) => {
                        // The question mark starts the beginning of API tokens.
                        eprintln!("Map error: {}", text.split("?").next().unwrap_or_default());
                    }

                    MapEvent::MapPositionClicked { position, .. } => {
                        let entry = self.entries.last_mut().unwrap();
                        match entry.key {
                            Some(AnnotationKey::Waypoint(waypoint_key)) => {
                                self.widget_system
                                    .get_waypoint_system_as_mut()
                                    .update_waypoint_position(waypoint_key, position)
                                    .expect("Key should be present");
                                entry.position = position;
                            }
                            // This example registers no regions or paths.
                            Some(AnnotationKey::Region(_) | AnnotationKey::Path(_)) | None => {}
                        }
                    }
                }
            }

            Message::Toggled(index, checked) => {
                let Some(entry) = self.entries.get(index) else {
                    return;
                };
                let (position, name, key) = (entry.position, entry.name, entry.key);

                match (checked, key) {
                    (true, None) => {
                        let key = self
                            .widget_system
                            .get_waypoint_system_as_mut()
                            .add_way_point(
                                WaypointSymbol::Image(Bytes::from_static(ICON_DATA)),
                                position,
                                Some(name.to_string()),
                            );
                        self.entries[index].key = Some(key.into());
                    }
                    (false, Some(AnnotationKey::Waypoint(key))) => {
                        self.widget_system
                            .get_waypoint_system_as_mut()
                            .delete_waypoint(key)
                            .expect("Key not stored");
                        self.entries[index].key = None;
                        // The flag lived on the way point and is gone with it.
                        self.entries[index].flagged = false;
                        // The way point is gone, so a pick that pointed at it is stale.
                        if self.selected == Some(key.into()) {
                            self.selected = None;
                        }
                    }
                    // This example registers no regions or paths.
                    (false, Some(AnnotationKey::Region(_) | AnnotationKey::Path(_))) => {}
                    // Already in the state the checkbox asks for.
                    (true, Some(_)) | (false, None) => {}
                }
            }

            // The row may not be registered with the map system, so the focus goes to
            // the position the application keeps rather than to the key.
            Message::Focus(index) => {
                let Some(entry) = self.entries.get(index) else {
                    return;
                };
                self.focus_all(entry.position.into());
            }

            Message::Flagged(index, flagged) => {
                let Some((key, flag)) = self
                    .entries
                    .get(index)
                    .and_then(|entry| Some((entry.key?, entry.flag)))
                else {
                    return;
                };
                match key {
                    AnnotationKey::Waypoint(key) => {
                        self.widget_system
                            .get_waypoint_system_as_mut()
                            .set_flag(key, flagged.then_some(flag))
                            .expect("Should be present");
                    }
                    // This example registers no regions or paths.
                    AnnotationKey::Region(_) | AnnotationKey::Path(_) => {}
                }
                self.entries[index].flagged = flagged;
            }
        }
    }

    /// Moves all maps to `target`: the left map jumps, the right one animates. For a
    /// position the hard focus keeps the zoom level, just like the soft one ends on it.
    fn focus_all(&mut self, target: FocusTarget) {
        self.widget_system
            .jump_to(self.widget_ids[0], target.clone());
        self.widget_system.animate_to(self.widget_ids[1], target);
    }

    /// Returns the visual representation of this map element, that can be included in a view
    /// command and can generate events.
    fn get_map_element(&self, widget_id: u32) -> Element<'_, Message> {
        let map: Element<'_, Message> = self
            .widget_system
            .canvas(widget_id)
            .map(|cmd| Message::Widget(cmd.into()));

        container(map)
            .padding(10)
            .width(FillPortion(2))
            .height(Fill)
            .style(|theme| container::Style {
                border: iced::Border {
                    color: theme.extended_palette().background.strong.color,
                    width: 10.0,
                    radius: 4.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// One row of the list: the name, the focus button, and the checkbox that
    /// registers the way point.
    fn waypoint_row(&self, index: usize, entry: &WaypointEntry) -> Element<'_, Message> {
        let is_selected = entry.key.is_some() && entry.key == self.selected;
        let label = text(entry.name)
            .style(if is_selected {
                text::primary
            } else {
                text::base
            })
            .width(Fill)
            .wrapping(Wrapping::WordOrGlyph);

        // Only a registered way point can carry a flag.
        let flag = checkbox(entry.flagged).label("Flag").on_toggle_maybe(
            entry
                .key
                .map(|_| move |flagged| Message::Flagged(index, flagged)),
        );

        let toggle = checkbox(entry.key.is_some())
            .on_toggle(move |checked| Message::Toggled(index, checked));

        let focus = button(text("Focus")).on_press(Message::Focus(index));

        row![label, focus, flag, toggle]
            .align_y(Alignment::Center)
            .spacing(10)
            .into()
    }

    fn middle_column(&self) -> Element<'_, Message> {
        let head_line = text("Way points:")
            .style(text::primary)
            .width(Fill)
            .align_x(Alignment::Center);
        let head_container = container(head_line).padding(20);

        let list = self
            .entries
            .iter()
            .enumerate()
            .fold(column![].spacing(10), |list, (index, entry)| {
                list.push(self.waypoint_row(index, entry))
            });

        column![head_container, list]
            .padding(10)
            .width(FillPortion(1))
            .height(Fill)
            .into()
    }

    fn view(&self) -> Element<'_, Message> {
        row![
            self.get_map_element(self.widget_ids[0]),
            self.middle_column(),
            self.get_map_element(self.widget_ids[1])
        ]
        .into()
    }
}

pub fn main() -> iced::Result {
    iced::application(
        WaypointApplication::boot,
        WaypointApplication::update,
        WaypointApplication::view,
    )
    .theme(Theme::TokyoNight)
    .centered()
    .window_size(Size {
        width: 512.0 * 3.0,
        height: 512.0,
    })
    .run()
}
