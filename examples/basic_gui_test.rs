use iced::widget::space::vertical;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Fill, FillPortion, Size, Task, Theme};
use map_iced::gui_system::map_widget_system::{MapEvent, MapWidgetMessage, MapWidgetSystem};
use map_iced::gui_system::tile_cache_construction::TileCacheConfig;

struct BasicApplication {
    widget_system: MapWidgetSystem,
    widget_ids: [u32; 2],
    error_text: String,
}

#[derive(Debug, Clone)]
enum Message {
    Widget(MapWidgetMessage),
    /// User message, when we want to retry failed tiles.
    RetryFailedTiles,
}

/// The configuration data to load.
const CONFIG_DATA: &str = include_str!("assets/osm.json");

impl BasicApplication {
    pub fn boot() -> (BasicApplication, Task<Message>) {
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
        (
            Self {
                widget_system,
                widget_ids,
                error_text: "".to_string(),
            },
            task.map(Message::Widget),
        )
    }

    fn update(&mut self, message: Message) {
        // println!("New message: {:?}", message);

        match message {
            Message::Widget(m) => {
                let mut errors = String::new();
                // This example does not deal with way points or focusing or map selection.
                if let Some(MapEvent::Error(text)) = self.widget_system.process_message(m) {
                    errors.push_str(&text);
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
        }
    }

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

        let retry = if self.widget_system.number_of_tiles_failed() == 0 {
            button("Retry")
        } else {
            button("Retry").on_press(Message::RetryFailedTiles)
        };
        let button_container = container(retry).align_x(Alignment::Center).width(Fill);

        column![head_container, message, vertical(), button_container]
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
