use std::path::PathBuf;
use iced::{Element, Task, Theme, Fill};
use iced::widget::{canvas, Column, container};
use map_iced::gui_system::high_level_tile_cache::TileCache;
use map_iced::gui_system::tile_cache_construction::{generate_debug_tile_cache, CachingDirectory};
use map_iced::tile_cache::cache_core::CachingResultMessage;
use map_iced::tile_cache::web_requester::DummyRequester;
use tokio_stream::wrappers::ReceiverStream;
use map_iced::gui_system::map_widget::MapInteractionCommand;
use map_iced::gui_system::map_widget_system::{MapWidgetMessage, MapWidgetSystem};
use map_iced::gui_system::math_coordinates::BoundingRectangle;

struct BasicApplication {
    widget_system : MapWidgetSystem<DummyRequester>,
    widget_id : u32,
}

#[derive(Debug, Clone)]
enum Message {
    WidgetMessage(MapWidgetMessage),
}

impl BasicApplication {
    pub fn boot() -> (BasicApplication,Task<Message>)  {
        let mut cache = generate_debug_tile_cache(CachingDirectory::FullyConstructed(PathBuf::from("transient")), 100_000).unwrap();
        let receiver = cache.get_receiver().unwrap();
        let mut widget_system = MapWidgetSystem::new(cache);
        let widget_id = widget_system.request_new_widget();
        let task = Task::run(ReceiverStream::new(receiver),  |x| Message::WidgetMessage(MapWidgetMessage::CachingResultMessage(x)));

        (Self {
            widget_system,
            widget_id,
        }, task)
    }

    fn update(&mut self, message : Message) {
        
        println!("New message: {:?}", message);


        match message {
            Message::WidgetMessage(m) => {self.widget_system.process_message(m)},
        };
        
    }

    fn view(&self) -> Element<'_, Message> {
        let map_canvas = canvas(self.widget_system.get_widget_access(self.widget_id).unwrap())
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
        .run()
}