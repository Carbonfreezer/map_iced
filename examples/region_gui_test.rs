//! Region example. Two way points, one region of each shape and two bicycle tracks
//! sit around Trier, all registered from the start. The tracks are GPX files of a
//! Garmin cycling computer, read with the `gpx` crate. The middle column lists the
//! elements with a focus button each; the left map jumps to the element, the right
//! one animates. A region or track is fitted into the map, a way point keeps the
//! zoom level.
//!
//! Clicking an element on a map writes its name into the report box below the list,
//! for a region together with the distance from the click point to the region's
//! centre, for a track with the distance ridden from its start to the point on it
//! nearest to the click. Hovering a track shows the same text in its label: the
//! example answers every `AnnotationHovered` on a track with a new description.
//! The way point "Trier Cathedral" lies inside the cordon circle and
//! is reported as the way point, since way points are drawn and hit on top of
//! regions, with the tracks in between.
//!
//! The button above the list switches the scale bars and the distance texts
//! between metric and imperial units.
//!
//! The cordon circle is flagged, so it gets a direction arrow while off screen, e.g.
//! after focusing the harbour of Mertert. Clicking the arrow focuses all maps on it.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Color, Element, Fill, FillPortion, Size, Task, Theme};
use map_iced::annotation_system::annotation_support::{AnnotationKey, DirectionFlag};
use map_iced::annotation_system::path_system::PathError;
use map_iced::annotation_system::region_system::{RegionShape, RegionStyle};
use map_iced::annotation_system::waypoint_system::WaypointSymbol;
use map_iced::gui_system::map_widget_system::{MapEvent, MapWidgetMessage, MapWidgetSystem};
use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
use map_iced::gui_system::{DistanceUnits, LatitudeLongitude};

/// The configuration data to load.
const CONFIG_DATA: &str = include_str!("assets/osm.json");

/// The two bicycle tracks, each with the colour of its line.
const TRACKS: [(&[u8], Color); 2] = [
    (
        include_bytes!("assets/Longuich.gpx"),
        Color::from_rgb(0.65, 0.2, 0.9),
    ),
    (
        include_bytes!("assets/Ferschweiler.gpx"),
        Color::from_rgb(0.0, 0.65, 0.75),
    ),
];

/// The share of the map a focused region covers.
const REGION_FILL: f32 = 0.8;

/// The direction arrow of the cordon circle.
const FLAG: DirectionFlag = DirectionFlag {
    color: Color::from_rgb(0.9, 0.15, 0.15),
    priority: 0,
};

/// The color for the dome.
const DOME_COLOR: Color = Color::from_rgb(0.3, 0.8, 0.3);

/// The color for the fire station.
const FIRE_STATION_COLOR: Color = Color::from_rgb(0.8, 0.3, 0.3);

/// The position of the dome.
const DOME_POSITION: LatitudeLongitude = LatitudeLongitude::new(49.7554, 6.6436);

/// One row of the list.
struct Entry {
    name: String,
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
    Widget(MapWidgetMessage),
    /// Move the maps to the given element.
    Focus(AnnotationKey),
    /// Switch between metric and imperial units.
    ToggleUnits,
}

impl RegionApplication {
    pub fn boot() -> (RegionApplication, Task<Message>) {
        let (mut widget_system, task) = TileCacheConfig::from_json_str(CONFIG_DATA)
            .and_then(MapWidgetSystem::boot)
            .unwrap_or_else(|e| {
                eprintln!("{e}, showing the test pattern instead");
                MapWidgetSystem::boot(TileCacheConfig::test_pattern())
                    .expect("Tile cache could not be created")
            });
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
            task.map(Message::Widget),
        )
    }

    /// Registers the two way points, the three regions and the two tracks with the
    /// map system and returns the list rows for them.
    fn register_elements(widget_system: &mut MapWidgetSystem) -> Vec<Entry> {
        let mut entries = Vec::new();

        let waypoints = widget_system.get_waypoint_system_as_mut();
        for (name, position, color) in [
            ("Trier Cathedral", DOME_POSITION, DOME_COLOR),
            (
                "Konz ambulance station",
                LatitudeLongitude::new(49.7020, 6.5800),
                FIRE_STATION_COLOR,
            ),
        ] {
            let key = waypoints.add_way_point(
                WaypointSymbol::cross(color),
                position,
                Some(name.to_string()),
            );
            entries.push(Entry {
                name: name.to_string(),
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
                "Cathedral cordon",
                RegionShape::Circle {
                    center: DOME_POSITION,
                    radius_meter: 1_200.0,
                },
                red,
            ),
            (
                "Mertert harbour",
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
                "Gas plume Trier North",
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
                name: name.to_string(),
                key: key.into(),
            });
        }

        let AnnotationKey::Region(circle_key) = entries[2].key else {
            unreachable!("The third entry is the circle");
        };
        regions
            .set_flag(circle_key, Some(FLAG))
            .expect("The circle was just added");

        let paths = widget_system.get_path_system_as_mut();
        for (data, color) in TRACKS {
            let (name, positions) = read_track(data);
            let key = paths
                .add_path(positions, color, Some(name.clone()))
                .expect("The example tracks have many positions");
            entries.push(Entry {
                name,
                key: key.into(),
            });
        }

        entries
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Widget(m) => {
                let Some(event) = self.widget_system.process_message(m) else {
                    return;
                };
                match event {
                    MapEvent::AnnotationSelected {
                        key, click_point, ..
                    } => self.report = self.describe_pick(key, click_point),
                    // A track's label shows the distance ridden up to the point
                    // under the cursor, the same text as the report of a click.
                    MapEvent::AnnotationHovered {
                        key: key @ AnnotationKey::Path(_),
                        hover_point,
                        ..
                    } => {
                        let description = self.describe_pick(key, hover_point);
                        self.widget_system
                            .update_description(key, Some(description))
                            .expect("Hover events only come for present elements");
                    }
                    MapEvent::AnnotationHovered { .. } | MapEvent::FocusReached { .. } => {}
                    // An arrow click acts like the focus button: all maps follow.
                    MapEvent::ArrowClicked { key, .. } => self.focus_all(key),
                    MapEvent::Error(text) => {
                        // The question mark starts the beginning of API tokens.
                        eprintln!("Map error: {}", text.split("?").next().unwrap_or_default());
                    }
                    MapEvent::MapPositionClicked { .. } => {}
                }
            }
            Message::Focus(key) => self.focus_all(key),
            Message::ToggleUnits => {
                let units = match self.widget_system.distance_units() {
                    DistanceUnits::Metric => DistanceUnits::Imperial,
                    DistanceUnits::Imperial => DistanceUnits::Metric,
                };
                self.widget_system.set_distance_units(units);
            }
        }
    }

    /// The report text for a pick of `key` at `click_point`: the name, for a region
    /// the distance from the click point to its centre, and for a track the distance
    /// ridden from its start to the point on it nearest to the click.
    fn describe_pick(&self, key: AnnotationKey, click_point: LatitudeLongitude) -> String {
        let name = self
            .entries
            .iter()
            .find(|entry| entry.key == key)
            .map_or("Unknown element", |entry| &entry.name);
        match key {
            AnnotationKey::Region(_) => match self.widget_system.center(key) {
                Ok(center) => format!(
                    "{name}\nDistance to centre: {}",
                    self.widget_system
                        .distance_units()
                        .distance_text(center.get_distance_to(click_point))
                ),
                Err(_) => name.to_string(),
            },
            AnnotationKey::Path(path_key) => match self
                .widget_system
                .get_path_system()
                .closest_point_on_path(path_key, click_point)
            {
                Ok(location) => format!(
                    "{name}\nDistance from start: {}",
                    self.widget_system
                        .distance_units()
                        .distance_text(location.distance_from_start_meter)
                ),
                Err(PathError::KeyNotContained | PathError::TooFewPositions) => name.to_string(),
            },
            AnnotationKey::Waypoint(_) => name.to_string(),
        }
    }

    /// Moves all maps to the element `key`: the left map jumps, the right one
    /// animates.
    fn focus_all(&mut self, key: AnnotationKey) {
        let Ok(target) = self.widget_system.focus_target(key, Some(REGION_FILL)) else {
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

    /// One row of the list: the name and the focus button.
    fn entry_row<'a>(&'a self, entry: &'a Entry) -> Element<'a, Message> {
        let label = text(&entry.name)
            .width(Fill)
            .wrapping(Wrapping::WordOrGlyph);

        let focus = button(text("Focus")).on_press(Message::Focus(entry.key));

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
        let units = match self.widget_system.distance_units() {
            DistanceUnits::Metric => "Imperial units",
            DistanceUnits::Imperial => "Metric units",
        };
        let head_container = column![
            head_line,
            button(text(units)).on_press(Message::ToggleUnits)
        ]
        .align_x(Alignment::Center)
        .spacing(10)
        .padding(20);

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
            // The list scrolls rather than run into the report box on a small window.
            scrollable(list).height(FillPortion(7)),
            container(report).height(FillPortion(3))
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

/// Reads the first track of a GPX file: its name and the positions of all its
/// segments, joined into one line.
fn read_track(data: &[u8]) -> (String, Vec<LatitudeLongitude>) {
    let gpx = gpx::read(data).expect("The example tracks are valid GPX");
    let track = gpx
        .tracks
        .into_iter()
        .next()
        .expect("The example files hold a track");
    let positions = track
        .segments
        .iter()
        .flat_map(|segment| &segment.points)
        .map(|point| {
            let point = point.point();
            LatitudeLongitude::new(point.y(), point.x())
        })
        .collect();
    (track.name.unwrap_or_else(|| "Track".to_string()), positions)
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
        width: 666.0 * 3.0,
        height: 666.0,
    })
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Garmin writes its own extensions into every track point, heart rate and
    /// temperature; the parser has to get past them.
    #[test]
    fn example_tracks_parse() {
        let counts = TRACKS.map(|(data, _)| read_track(data).1.len());
        assert_eq!(counts, [2237, 18103]);
    }
}
