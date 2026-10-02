use iced::widget::space::vertical;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Color, Element, Fill, FillPortion, Size, Task, Theme};
use map_iced::Bytes;
use map_iced::annotation_system::waypoint_system::WaypointSymbol;
use map_iced::gui_system::internal_math::LatitudeLongitude;
use map_iced::gui_system::map_widget_system::{MapEvent, MapWidgetMessage, MapWidgetSystem};
use map_iced::gui_system::tile_cache_construction::{TileCacheConfig, tile_cache_debug_default};

struct BasicApplication {
    widget_system: MapWidgetSystem,
    widget_ids: [u32; 2],
    error_text: String,
    /// Description of the way point the user selected, empty when nothing is selected.
    selection_text: String,
}

#[derive(Debug, Clone)]
enum Message {
    WidgetMessage(MapWidgetMessage),
    /// User message, when we want to retry failed tiles.
    RetryFailedTiles,
    /// User message, when the selection display should be reset.
    ClearSelection,
}

/// The configuration data to load.
const CONFIG_DATA: &str = include_str!("../osm.json");

/// The icon we use for one of the way points.
const ICON_DATA: &[u8] = include_bytes!("../assets/Icon.png");

impl BasicApplication {
    pub fn boot() -> (BasicApplication, Task<Message>) {
        let cache = TileCacheConfig::from_json_str(CONFIG_DATA)
            .and_then(TileCacheConfig::build)
            .or_else(|e| {
                eprintln!("Konfigurationsfehler: {e}");
                tile_cache_debug_default()
            })
            .expect("Should not be possible to reach");

        let (mut widget_system, task) = MapWidgetSystem::boot(cache);
        // TODO: Local hack.
        widget_system.get_waypoint_as_mut().add_way_point(WaypointSymbol::Cross(Color::WHITE), LatitudeLongitude::new(50.0, 7.0),Some("Test Text".to_string()));
        widget_system.get_waypoint_as_mut().add_way_point(
            WaypointSymbol::Image(Bytes::from_static(ICON_DATA)),
            LatitudeLongitude::new(50.0, 7.01),
            Some("Second Text".to_string()),
        );
        let widget_ids = [
            widget_system.request_new_widget(),
            widget_system.request_new_widget(),
        ];
        (
            Self {
                widget_system,
                widget_ids,
                error_text: "".to_string(),
                selection_text: "".to_string(),
            },
            task.map(Message::WidgetMessage),
        )
    }

    fn update(&mut self, message: Message) {
        // println!("New message: {:?}", message);

        match message {
            Message::WidgetMessage(m) => {
                let mut errors = String::new();
                for event in self.widget_system.process_message(m) {
                    match event {
                        MapEvent::Error(text) => errors.push_str(&text),
                        MapEvent::WaypointSelected { key, .. } => {
                            self.selection_text = self
                                .widget_system
                                .get_waypoints()
                                .get_waypoint_info(key)
                                .and_then(|point| point.description.clone())
                                .unwrap_or_else(|| "<no description>".to_string());
                        }
                    }
                }
                // The question mark starts the beginning of API tokens and is very long.
                let new_message = errors.split("?").collect::<Vec<&str>>()[0].to_string();
                if !new_message.is_empty() || self.widget_system.number_of_tiles_failed() == 0 {
                    self.error_text = new_message;
                }
            }
            Message::RetryFailedTiles => {
                self.error_text = "".to_string();
                self.widget_system.retry_failed_tiles();
            }
            // The map system reports a selection and then forgets about it, so keeping
            // it and resetting it is up to us.
            Message::ClearSelection => self.selection_text.clear(),
        }
    }

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

    fn middle_column(&self) -> Element<'_, Message> {
        let head_line = text("Status:")
            .style(text::primary)
            .width(Fill)
            .align_x(Alignment::Center);
        let head_container = container(head_line).padding(20);
        let message = if self.error_text.is_empty() {
            text("Ok").style(text::base)
        } else {
            text(self.error_text.as_str()).style(text::danger)
        }
        .width(Fill)
        .wrapping(Wrapping::WordOrGlyph);

        let selection = text(format!("Selected: {}", self.selection_text))
            .style(text::base)
            .width(Fill)
            .wrapping(Wrapping::WordOrGlyph);

        let clear = if self.selection_text.is_empty() {
            button("Clear selection")
        } else {
            button("Clear selection").on_press(Message::ClearSelection)
        };
        let clear_container = container(clear).align_x(Alignment::Center).width(Fill);

        let retry = if self.widget_system.number_of_tiles_failed() == 0 {
            button("Retry")
        } else {
            button("Retry").on_press(Message::RetryFailedTiles)
        };
        let button_container = container(retry).align_x(Alignment::Center).width(Fill);

        column![
            head_container,
            message,
            selection,
            clear_container,
            vertical(),
            button_container
        ]
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
        BasicApplication::boot,
        BasicApplication::update,
        BasicApplication::view,
    )
    .theme(Theme::TokyoNight)
    .centered()
    .window_size(Size {
        width: 512.0 * 3.0,
        height: 512.0,
    })
    .run()
}
