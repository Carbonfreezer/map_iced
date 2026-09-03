use std::path::PathBuf;
use iced::{Task, Theme};
use iced::widget::Column;
use map_iced::gui_system::high_level_tile_cache::TileCache;
use map_iced::gui_system::tile_cache_construction::{generate_debug_tile_cache, CachingDirectory};
use map_iced::tile_cache::cache_core::CachingResultMessage;
use map_iced::tile_cache::web_requester::DummyRequester;
use tokio_stream::wrappers::ReceiverStream;
use map_iced::gui_system::math_coordinates::BoundingRectangle;

struct BasicApplication {
    cache : TileCache<DummyRequester>,
}

#[derive(Debug, Clone)]
enum Message {
    TileMapMessage(CachingResultMessage),
}

impl BasicApplication {
    pub fn boot() -> (BasicApplication,Task<Message>)  {
        let mut cache = generate_debug_tile_cache(CachingDirectory::FullyConstructed(PathBuf::from("transient")), 100_000).unwrap();
        cache.register_new_interest_area(0, BoundingRectangle { x_min: 0, y_min:0, width: 3, height: 3}, 5);
        cache.register_new_interest_area(1, BoundingRectangle { x_min: 0, y_min:0, width: 3, height: 3}, 5);
        let receiver = cache.get_receiver().unwrap();
        let task = Task::run(ReceiverStream::new(receiver), Message::TileMapMessage);
        (Self {
            cache,
        }, task)
    }

    fn update(&mut self, message : Message) {
        
        println!("New message: {:?}", message);
        
        match message {
            Message::TileMapMessage(m) => {self.cache.process_caching_message(m.clone())},
        };

        let messages = self.cache.drain_result_messages();

        for x in messages {
            println!("Caching result message: {:?}", x);
        }

        
    }

    fn view(&self) -> Column<'_, Message> {
        Column::new()
    }
}


pub fn main() -> iced::Result {
    iced::application(BasicApplication::boot, BasicApplication::update,BasicApplication::view)
        .theme(Theme::TokyoNight)
        .centered()
        .run()
}