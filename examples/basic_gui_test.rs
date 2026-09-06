use iced::widget::{button, row};
use iced::{Element, Fill, Size, Task, Theme};
use map_iced::gui_system::map_widget_system::{MapWidgetMessage, MapWidgetSystem};
use map_iced::gui_system::tile_cache_construction::generate_from_config_default;

struct BasicApplication {
    widget_system: MapWidgetSystem,
    widget_ids: [u32;2],
}

#[derive(Debug, Clone)]
enum Message {
    WidgetMessage(MapWidgetMessage),
}

impl BasicApplication {
    pub fn boot() -> (BasicApplication, Task<Message>) {
        let cache = generate_from_config_default("mapbox.json").unwrap();

        let (mut widget_system, task) = MapWidgetSystem::boot(cache);
        let widget_ids = [widget_system.request_new_widget(), widget_system.request_new_widget()];
        (
            Self {
                widget_system,
                widget_ids,
            },
            task.map(Message::WidgetMessage),
        )
    }

    fn update(&mut self, message: Message) {
        // println!("New message: {:?}", message);

        match message {
            Message::WidgetMessage(m) => self.widget_system.process_message(m),
        };
    }

    fn get_map_element(&self, widget_id: u32) -> Element<'_, Message> {
        let map_canvas = self
            .widget_system
            .canvas(widget_id)
            .width(Fill)
            .height(Fill);

        // Canvas<MapWidget, MapInteractionCommand> -> Element<MapInteractionCommand> -> Element<Message>
        Element::from(map_canvas).map(|cmd| Message::WidgetMessage(cmd.into()))
    }

    fn view(&self) -> Element<'_, Message> {
        row![self.get_map_element(self.widget_ids[0]), self.get_map_element(self.widget_ids[1])].into()
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
        width: 768.0 * 2.0,
        height: 768.0,
    })
    .run()
}
