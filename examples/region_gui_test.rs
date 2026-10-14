//! Region example. Two way points and one region of each shape sit around Trier, all
//! registered from the start. The middle column lists them with a focus button each;
//! the left map jumps to the element, the right one animates. A region is fitted into
//! the map, a way point keeps the zoom level.
//!
//! Clicking an element on a map writes its name into the report box below the list,
//! for a region together with the distance from the click point to the region's
//! centre. The way point "Trier Dom" lies inside the cordon circle and is reported
//! as the way point, since way points are drawn and hit on top of regions.
//!
//! The cordon circle is flagged, so it gets a direction arrow while off screen, e.g.
//! after focusing the harbour of Mertert. Clicking the arrow focuses all maps on it.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Color, Element, Fill, FillPortion, Size, Task, Theme};
use map_iced::annotation_system::annotation_support::AnnotationKey;
use map_iced::annotation_system::region_system::{RegionShape, RegionStyle};
use map_iced::annotation_system::waypoint_system::{DirectionFlag, WaypointSymbol};
use map_iced::gui_system::latitude_longitude::LatitudeLongitude;
use map_iced::gui_system::map_widget_system::{MapEvent, MapWidgetMessage, MapWidgetSystem};
use map_iced::gui_system::tile_cache_construction::{TileCacheConfig, tile_cache_debug_default};

/// The configuration data to load.
const CONFIG_DATA: &str = include_str!("../osm.json");


/// The share of the map a focused region covers.
const REGION_FILL: f32 = 0.8;

/// The direction arrow of the cordon circle.
const FLAG: DirectionFlag = DirectionFlag {
    color: Color::from_rgb(0.9, 0.15, 0.15),
    priority: 0,
};


/// The color for the dome.
const DOME_COLOR : Color = Color::from_rgb(0.3, 0.8, 0.3);

/// The color for the fire station.
const FIRE_STATION_COLOR : Color = Color::from_rgb(0.8, 0.3, 0.3);

/// The position of the dome.
const DOME_POSITION: LatitudeLongitude = LatitudeLongitude::new(49.7554, 6.6436);

/// One row of the list.
struct Entry {
    name: &'static str,
    key: AnnotationKey,
}

struct RegionApplication {
    widget_system: MapWidgetSystem,
    widget_ids: [u32; 2],
    entries: Vec<Entry>,
    /// What the report box shows about the last pick.
    report: String,
}

/// The overall message system in this application.
#[derive(Debug, Clone)]
enum Message {
    /// An internal message from the widget system.
    WidgetMessage(MapWidgetMessage),
    /// Move the maps to the given element.
    Focus(AnnotationKey),
}

impl RegionApplication {
    pub fn boot() -> (RegionApplication, Task<Message>) {
        let cache = TileCacheConfig::from_json_str(CONFIG_DATA)
            .and_then(TileCacheConfig::build)
            .or_else(|e| {
                eprintln!("Konfigurationsfehler: {e}");
                tile_cache_debug_default()
            })
            .expect("Should not be possible to reach");

        let (mut widget_system, task) = MapWidgetSystem::boot(cache);
        let widget_ids = [
            widget_system.request_new_widget(),
            widget_system.request_new_widget(),
        ];
        let entries = Self::register_elements(&mut widget_system);

        (
            Self {
                widget_system,
                widget_ids,
                entries,
                report: "Click an element on a map.".to_string(),
            },
            task.map(Message::WidgetMessage),
        )
    }

    /// Registers the two way points and the three regions with the map system and
    /// returns the list rows for them.
    fn register_elements(widget_system: &mut MapWidgetSystem) -> Vec<Entry> {
        let mut entries = Vec::new();

        let waypoints = widget_system.get_waypoint_system_as_mut();
        for (name, position, color) in [
            ("Trier Dom", DOME_POSITION.clone(), DOME_COLOR),
            (
                "Rettungswache Konz",
                LatitudeLongitude::new(49.7020, 6.5800),
                FIRE_STATION_COLOR,
            ),
        ] {
            let key = waypoints.add_way_point(
                WaypointSymbol::Cross(color),
                position,
                Some(name.to_string()),
            );
            entries.push(Entry {
                name,
                key: key.into(),
            });
        }

        let regions = widget_system.get_region_system_as_mut();
        let red = RegionStyle {
            fill: Color::from_rgba(0.9, 0.15, 0.15, 0.25),
            border: Color::from_rgb(0.9, 0.15, 0.15),
        };
        let blue = RegionStyle {
            fill: Color::from_rgba(0.2, 0.4, 0.95, 0.25),
            border: Color::from_rgb(0.2, 0.4, 0.95),
        };
        let yellow = RegionStyle {
            fill: Color::from_rgba(0.95, 0.8, 0.1, 0.3),
            border: Color::from_rgb(0.95, 0.7, 0.1),
        };
        let shapes = [
            // The centre is a little off the cathedral, so that a click on the way
            // point and one next to it in the circle are told apart.
            (
                "Absperrkreis Dom",
                RegionShape::Circle {
                    center: DOME_POSITION.clone(),
                    radius_meter: 1_200.0,
                },
                red,
            ),
            (
                "Hafen Mertert",
                RegionShape::ConvexHull {
                    points: vec![
                        LatitudeLongitude::new(49.7120, 6.4700),
                        LatitudeLongitude::new(49.7100, 6.4920),
                        LatitudeLongitude::new(49.6980, 6.4970),
                        LatitudeLongitude::new(49.6940, 6.4780),
                        LatitudeLongitude::new(49.7030, 6.4650),
                    ],
                },
                blue,
            ),
            // Gas from a leak, carried north east by a south west wind.
            (
                "Gaswolke Trier-Nord",
                RegionShape::Sector {
                    apex: LatitudeLongitude::new(49.7750, 6.6550),
                    radius_meter: 2_500.0,
                    heading_degree: 50.0,
                    opening_degree: 40.0,
                },
                yellow,
            ),
        ];
        for (name, shape, style) in shapes {
            let key = regions
                .add_region(shape, style, Some(name.to_string()))
                .expect("The example shapes span an area");
            entries.push(Entry {
                name,
                key: key.into(),
            });
        }

        let AnnotationKey::Region(circle_key) = entries[2].key else {
            unreachable!("The third entry is the circle");
        };
        regions
            .set_flag(circle_key, Some(FLAG))
            .expect("The circle was just added");

        entries
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::WidgetMessage(m) => {
                if let Some(event) = self.widget_system.process_message(m) {
                    match event {
                        MapEvent::AnnotationSelected {
                            key, click_point, ..
                        } => self.report = self.describe_pick(key, click_point),
                        MapEvent::FocusReached { .. } => {}
                        // An arrow click acts like the focus button: all maps follow.
                        MapEvent::ArrowClicked { key, .. } => self.focus_all(key),
                        MapEvent::Error(text) => {
                            // The question mark starts the beginning of API tokens.
                            eprintln!(
                                "Kartenfehler: {}",
                                text.split("?").next().unwrap_or_default()
                            );
                        }
                        MapEvent::MapPositionClicked { .. } => {}
                    }
                }
            }
            Message::Focus(key) => self.focus_all(key),
        }
    }

    /// The report text for a pick of `key` at `click_point`: the name, and for a
    /// region the distance from the click point to its centre.
    fn describe_pick(&self, key: AnnotationKey, click_point: LatitudeLongitude) -> String {
        let name = self
            .entries
            .iter()
            .find(|entry| entry.key == key)
            .map_or("Unknown element", |entry| entry.name);
        match (key, self.widget_system.center(key)) {
            (AnnotationKey::Region(_), Some(center)) => format!(
                "{name}\nDistance to centre: {}",
                format_distance(center.get_distance_to(click_point))
            ),
            (AnnotationKey::Region(_), None) | (AnnotationKey::Waypoint(_), _) => name.to_string(),
        }
    }

    /// Moves all maps to the element `key`: the left map jumps, the right one
    /// animates.
    fn focus_all(&mut self, key: AnnotationKey) {
        let Some(target) = self.widget_system.focus_target(key, Some(REGION_FILL)) else {
            return;
        };
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
            .map(|cmd| Message::WidgetMessage(cmd.into()));

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

    /// One row of the list: the name and the focus button.
    fn entry_row(&self, entry: &Entry) -> Element<'_, Message> {
        let label = text(entry.name).width(Fill).wrapping(Wrapping::WordOrGlyph);

        let focus = button(text("Fokus")).on_press(Message::Focus(entry.key));

        row![label, focus]
            .align_y(Alignment::Center)
            .spacing(10)
            .into()
    }

    fn middle_column(&self) -> Element<'_, Message> {
        let head_line = text("Elements:")
            .style(text::primary)
            .width(Fill)
            .align_x(Alignment::Center);
        let head_container = container(head_line).padding(20);

        let list = self
            .entries
            .iter()
            .fold(column![].spacing(10), |list, entry| {
                list.push(self.entry_row(entry))
            });

        let report = container(text(&self.report).wrapping(Wrapping::WordOrGlyph))
            .padding(10)
            .width(Fill)
            .height(Fill)
            .style(container::bordered_box);

        column![
            head_container,
            container(list).height(FillPortion(3)),
            container(report).height(FillPortion(2))
        ]
        .padding(10)
        .spacing(10)
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

/// A distance in metres as text, in kilometres from one kilometre on.
fn format_distance(meter: f64) -> String {
    if meter < 1_000.0 {
        format!("{meter:.0} m")
    } else {
        format!("{:.2} km", meter / 1_000.0)
    }
}

pub fn main() -> iced::Result {
    iced::application(
        RegionApplication::boot,
        RegionApplication::update,
        RegionApplication::view,
    )
    .theme(Theme::TokyoNight)
    .centered()
    .window_size(Size {
        width: 512.0 * 3.0,
        height: 512.0,
    })
    .run()
}
