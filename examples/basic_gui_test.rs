use std::path::PathBuf;
use iced::{Element, Task, Theme, Fill, Size};
use iced::widget::container;
use map_iced::gui_system::tile_cache_construction::{generate_debug_tile_cache, CachingDirectory};
use map_iced::gui_system::map_widget_system::{MapWidgetMessage, MapWidgetSystem};

struct BasicApplication {
    widget_system : MapWidgetSystem,
    widget_id : u32,
}

#[derive(Debug, Clone)]
enum Message {
    WidgetMessage(MapWidgetMessage),
}

impl BasicApplication {
    pub fn boot() -> (BasicApplication,Task<Message>)  {
        let cache = generate_debug_tile_cache(CachingDirectory::FullyConstructed(PathBuf::from("transient")), 100_000).unwrap();

        let (mut widget_system, task) = MapWidgetSystem::boot(cache);
        let widget_id = widget_system.request_new_widget();
        (Self { widget_system, widget_id }, task.map(Message::WidgetMessage))
    }

    fn update(&mut self, message : Message) {
        
        // println!("New message: {:?}", message);


        match message {
            Message::WidgetMessage(m) => {self.widget_system.process_message(m)},
        };
        
    }

    fn view(&self) -> Element<'_, Message> {
        let map_canvas = self.widget_system.canvas(self.widget_id)
            .width(Fill)
            .height(Fill);

        // Canvas<MapWidget, MapInteractionCommand> -> Element<MapInteractionCommand> -> Element<Message>
        let mapped: Element<'_, Message> = Element::from(map_canvas)
            .map(|cmd| Message::WidgetMessage(cmd.into()));

        container(mapped).into()
    }
}


pub fn main() -> iced::Result {
    iced::application(BasicApplication::boot, BasicApplication::update,BasicApplication::view)
        .theme(Theme::TokyoNight)
        .centered()
        .window_size(Size{width:768.0, height:768.0})
        .run()
}