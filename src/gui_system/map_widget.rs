//! This contains the core map widget.

use crate::gui_system::high_level_tile_cache::TilesToDraw;
use crate::gui_system::math_coordinates::{
    BoundingRectangle, DrawingPositionConverter, LatitudeLongitude, TileCoordinates,
};
use iced::mouse::{Cursor, Interaction};
use iced::widget::canvas::{Cache, Geometry};
use iced::widget::{Action, canvas};
use iced::{Event, Rectangle, Renderer, Theme};

/// These become the interaction commands with the rest of the system later on.
#[derive(Debug, Clone)]
pub struct MapInteractionCommand {
    pub client_id: u32,
    pub command: SpecificInteractionCommand,
}

/// Focal point info.
#[derive(Debug, Clone)]
pub struct FocalPoint {
    pub position: LatitudeLongitude,
    pub continuous_zoom_level: f32,
}

#[derive(Debug, Clone)]
pub enum SpecificInteractionCommand {
    /// We want to set the focal point as latitude longitude and the zoom level.
    SetFocalPoint(FocalPoint, Rectangle),
}

/// The internal state for mouse processing.
#[derive(Default)]
pub struct InteractionState {
    is_initialized: bool,
}

/// The widget used for rendering a tile.
pub struct MapWidget {
    /// The drawing cache for the different tiles.
    tile_drawing_cache: Cache,
    /// The tiles we need to draw.
    drawing_tiles: Vec<TilesToDraw>,
    /// The client id we belong to.
    client_id: u32,
    /// The position converter.
    position_converter: Option<DrawingPositionConverter>,
}

impl MapWidget {
    /// Creates a new widget from the client id.
    pub fn new(client_id: u32) -> Self {
        Self {
            tile_drawing_cache: Default::default(),
            drawing_tiles: vec![],
            client_id,
            position_converter: None,
        }
    }

    /// Sets the drawing tiles from the our
    pub fn set_drawing_tiles(&mut self, drawing_tiles: Vec<TilesToDraw>) {
        self.drawing_tiles = drawing_tiles;
        self.tile_drawing_cache.clear();
    }

    /// Sets the focal points and gets the bounding rectangle for subscription
    pub fn set_zoom_level_and_get_bounding_rect(
        &mut self,
        focal_point: FocalPoint,
        bounds: Rectangle,
    ) -> Option<BoundingRectangle> {
        self.position_converter = Some(DrawingPositionConverter::new(
            &focal_point.position,
            focal_point.continuous_zoom_level,
            &bounds,
        ));
        self.position_converter
            .as_ref()
            .unwrap()
            .bounding_rectangle()
    }
}

impl canvas::Program<MapInteractionCommand> for MapWidget {
    type State = InteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        _event: &Event,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Option<Action<MapInteractionCommand>> {
        if (state.is_initialized) {
            None
        } else {
            state.is_initialized = true;
            Some(Action::publish(MapInteractionCommand {
                client_id: self.client_id,
                command: SpecificInteractionCommand::SetFocalPoint(
                    FocalPoint {
                        position: LatitudeLongitude::new(49.75, 6.63),
                        continuous_zoom_level: 12.0,
                    },
                    bounds,
                ),
            }))
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        vec![]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Interaction {
        Interaction::None
    }
}
