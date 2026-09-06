use iced::widget::space::vertical;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Color, Element, Fill, FillPortion, Size, Task, Theme};
use map_iced::gui_system::map_widget_system::{
    MapWidgetMessage, MapWidgetSystem, StatusUpdateInformation,
};
use map_iced::gui_system::tile_cache_construction::generate_from_config_default;
use map_iced::tile_cache::cache_core::CachingResultMessage;

struct BasicApplication {
    widget_system: MapWidgetSystem,
    widget_ids: [u32; 2],
    error_text: String,
}

#[derive(Debug, Clone)]
enum Message {
    WidgetMessage(MapWidgetMessage),
}

impl BasicApplication {
    pub fn boot() -> (BasicApplication, Task<Message>) {
        let cache = generate_from_config_default("mapbox.json").unwrap();

        let (mut widget_system, task) = MapWidgetSystem::boot(cache);
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
            task.map(Message::WidgetMessage),
        )
    }

    fn update(&mut self, message: Message) {
        // println!("New message: {:?}", message);

        let work_list = match message {
            Message::WidgetMessage(m) => self.widget_system.process_message(m),
        };
        let mut new_error = work_list
            .into_iter()
            .filter_map(|x| match x {
                StatusUpdateInformation::ClearErrorMessages => Some(" ".to_string()), // Simply return an empty string to cancel the error.
                StatusUpdateInformation::ErrorText(text) => Some(text + "\n"),
            })
            .collect::<String>();
        // The question mark starts the beginning of API tokens and is very long.
        new_error = new_error.split("?").collect::<Vec<&str>>()[0].to_string();
        if !new_error.is_empty() {
            self.error_text = new_error.trim().to_string();
        }
    }

    fn get_map_element(&self, widget_id: u32) -> Element<'_, Message> {
        let map_canvas = self
            .widget_system
            .canvas(widget_id)
            .width(Fill)
            .height(Fill);

        let map: Element<'_, Message> =
            Element::from(map_canvas).map(|cmd| Message::WidgetMessage(cmd.into()));

        container(map)
            .padding(10)
            .width(FillPortion(2))
            .height(Fill)
            .style(|_theme| container::Style {
                border: iced::Border {
                    color: Color::from_rgb8(0x60, 0x60, 0x60),
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
        let message = text(self.error_text.as_str()).style(text::danger);

        let button = if self.error_text.is_empty() {
            button("Retry")
        } else {
            button("Retry").on_press(Message::WidgetMessage(
                MapWidgetMessage::CachingResultMessage(CachingResultMessage::Retry),
            ))
        };
        let button_container = container(button).align_x(Alignment::Center).width(Fill);
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
