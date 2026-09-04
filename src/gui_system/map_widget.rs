//! This contains the core map widget.

use iced::{Event, Rectangle, Renderer, Theme};
use iced::mouse::{Cursor, Interaction};
use iced::widget::{canvas, Action};
use iced::widget::canvas::{Cache, Geometry};
use crate::gui_system::high_level_tile_cache::TilesToDraw;

/// These become the interaction commands with the rest of the system later on.
#[derive(Debug, Clone)]
pub struct MapInteractionCommand {
    pub client_id: u32,
    pub command : u32   // TODO: Here comes an enum for the real command.
}


/// The internal state for mouse processing.
#[derive(Default)]
pub struct InteractionState { }


/// The widget used for rendering a tile.
pub struct MapWidget {
    /// The drawing cache for the different tiles.
    tile_drawing_cache : Cache,
    /// The tiles we need to draw.
    drawing_tiles: Vec<TilesToDraw>,
    /// The client id we belong to.
    client_id : u32,
}

impl MapWidget {

    /// Creates a new widget from the client id.
    pub fn new(client_id : u32) -> Self {
        Self {
            tile_drawing_cache : Default::default(),
            drawing_tiles : vec![],
            client_id,
        }
    }

    /// Sets the drawing tiles from the our
    pub fn set_drawing_tiles(&mut self, drawing_tiles: Vec<TilesToDraw>) {
        self.drawing_tiles = drawing_tiles;
        self.tile_drawing_cache.clear();
    }
}

impl canvas::Program<MapInteractionCommand> for MapWidget {
    type State = InteractionState;


    fn update(&self, _state: &mut Self::State, _event: &Event, _bounds: Rectangle, _cursor: Cursor) -> Option<Action<MapInteractionCommand>> {
        None
    }

    fn draw(&self, state: &Self::State, renderer: &Renderer, theme: &Theme, bounds: Rectangle, cursor: Cursor) -> Vec<Geometry<Renderer>> {
        vec![]
    }

    fn mouse_interaction(&self, _state: &Self::State, _bounds: Rectangle, _cursor: Cursor) -> Interaction {
        Interaction::None
    }
}

