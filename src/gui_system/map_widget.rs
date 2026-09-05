//! This contains the core map widget.

use iced::{Event, Rectangle, Renderer, Theme};
use iced::mouse::{Cursor, Interaction};
use iced::widget::{canvas, Action};
use iced::widget::canvas::{Cache, Geometry};
use crate::gui_system::high_level_tile_cache::TilesToDraw;
use crate::gui_system::math_coordinates::{LatitudeLongitude, TileCoordinates};

/// These become the interaction commands with the rest of the system later on.
#[derive(Debug, Clone)]
pub struct MapInteractionCommand {
    pub client_id: u32,
    pub command : SpecificInteractionCommand
}


#[derive(Debug, Clone)]
pub enum SpecificInteractionCommand {
    /// We want to set the zoom level  float.
    SetZoomLevel(f32),
    /// We want to set the focal point as latitude longitude.
    SetFocalPoint(LatitudeLongitude),
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
    /// The current zoom level we have as floating point from 0..19
    zoom_level: f32,
    /// The current focal point.
    focal_point: LatitudeLongitude,
}

impl MapWidget {

    /// Creates a new widget from the client id.
    pub fn new(client_id : u32) -> Self {
        Self {
            tile_drawing_cache : Default::default(),
            drawing_tiles : vec![],
            client_id,
            zoom_level : 12.0,
            focal_point :  LatitudeLongitude::new( 49.75, 6.63)
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

