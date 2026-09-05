//! This contains the core map widget.

use crate::gui_system::high_level_tile_cache::TilesToDraw;
use crate::gui_system::math_coordinates::{BoundingRectangle, DrawingPositionConverter, LatitudeLongitude, RectConversionError, MAXIMUM_ZOOM_LEVEL, TILE_SIZE_PIXEL};
use iced::advanced::image:: Image;
use iced::mouse::{Cursor, Interaction, ScrollDelta};
use iced::widget::canvas::{Cache, Geometry};
use iced::widget::{Action, canvas};
use iced::{mouse, window, Event, Rectangle, Renderer, Theme, Point};

/// The velocity we use for mouse scrolling.
const SCROLLING_SPEED: f32 = 0.05;

/// These become the interaction commands with the rest of the system later on.
#[derive(Debug, Clone)]
pub struct MapInteractionCommand {
    pub client_id: u32,
    pub command: SpecificInteractionCommand,
}

/// Focal point info.
#[derive(Debug, Clone, Copy)]
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
    /// Flags initialization.
    is_initialized: bool,
    /// Contains the last position, when the middle mouse button is pressed.
    drag_origin: Option<Point>
}



/// The widget used for rendering a tile.
pub struct MapWidget {
    tile_drawing_cache: Cache,
    /// Tiles for the current view, possibly still filling up.
    drawing_tiles: Vec<TilesToDraw>,
    /// Last complete set, kept as backdrop while the current one fills up.
    fallback_tiles: Vec<TilesToDraw>,
    client_id: u32,
    /// The view this widget currently shows. Source of truth for interaction.
    focal_point: FocalPoint,
    /// Derived from `focal_point` plus the canvas bounds, for rendering only.
    position_converter: Option<DrawingPositionConverter>
}


/// The rectangle that covers one tile.
const STANDARD_RECTANGLE : Rectangle = Rectangle {
    x: 0.0,
    y: 0.0,
    width: TILE_SIZE_PIXEL as f32,
    height: TILE_SIZE_PIXEL as f32,
};

impl MapWidget {
    /// Creates a new widget from the client id.
    pub fn new(client_id: u32, focal_point: FocalPoint) -> Self {
        Self {
            tile_drawing_cache: Default::default(),
            drawing_tiles: vec![],
            fallback_tiles: vec![],
            client_id,
            position_converter: None,
            focal_point,
        }
    }

    /// Rebuilds the converter for a new view and reports the tiles it needs.
    pub fn apply_focal_point(
        &mut self,
        focal_point: FocalPoint,
        bounds: Rectangle,
    ) -> Option<BoundingRectangle> {
        let (converter, rectangle) = DrawingPositionConverter::new(
            &focal_point.position,
            focal_point.continuous_zoom_level,
            &bounds,
        );
        let zoom_changed =
            self.position_converter.as_ref().map(|c| c.zoom()) != Some(converter.zoom());
        if zoom_changed {
            let previous = std::mem::take(&mut self.drawing_tiles);
            if !previous.is_empty() {
                self.fallback_tiles = previous;
            }
        }
        self.focal_point = focal_point;
        self.position_converter = Some(converter);
        self.tile_drawing_cache.clear();
        debug_assert!(!matches!(rectangle, Err(RectConversionError::NegativeSize)), "Negative size in rectangle detected.");
        rectangle.ok()
    }

    pub fn set_drawing_tiles(&mut self, drawing_tiles: Vec<TilesToDraw>) {
        self.drawing_tiles = drawing_tiles;
        self.tile_drawing_cache.clear();
    }

    fn publish(&self, focal_point: FocalPoint, bounds: Rectangle)
               -> Option<Action<MapInteractionCommand>>
    {
        Some(Action::publish(MapInteractionCommand {
            client_id: self.client_id,
            command: SpecificInteractionCommand::SetFocalPoint(focal_point, bounds),
        }))
    }
}

impl canvas::Program<MapInteractionCommand> for MapWidget {
    type State = InteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<MapInteractionCommand>> {
        if !state.is_initialized {
            state.is_initialized = true;
            return self.publish(self.focal_point, bounds);
        }

        match event {
            Event::Window(window::Event::Resized(_)) => self.publish(self.focal_point, bounds),

            Event::Mouse(mouse::Event::WheelScrolled { delta: ScrollDelta::Lines { y, .. } }) => {
                let zoom = (self.focal_point.continuous_zoom_level + y * SCROLLING_SPEED)
                    .clamp(0.0, MAXIMUM_ZOOM_LEVEL as f32);
                self.publish(FocalPoint { continuous_zoom_level: zoom, ..self.focal_point }, bounds)
            }

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                state.drag_origin = cursor.position_in(bounds);
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                state.drag_origin = None;
                None
            }

            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let origin = state.drag_origin?;
                let now = cursor.position_in(bounds)?;
                let converter = self.position_converter.as_ref()?;

                let delta = now - origin;
                if delta.x == 0.0 && delta.y == 0.0 {
                    return None;
                }
                state.drag_origin = Some(now);
                self.publish(
                    FocalPoint {
                        position: converter.get_new_coord_for_mouse_delta(delta),
                        ..self.focal_point
                    },
                    bounds,
                )
            }

            _ => None,
        }
    }


    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let Some(converter) = &self.position_converter else {
            return vec![];
        };
        let content = self.tile_drawing_cache.draw(renderer, bounds.size(), |frame| {
            for tile_and_pos in self.fallback_tiles.iter().chain(self.drawing_tiles.iter()) {
                let Some(draw) = converter.get_draw_instruction(tile_and_pos.position.into()) else {
                    continue;
                };
                frame.with_save(|frame| {
                    frame.translate(draw.offset);
                    frame.scale(draw.scale);
                    frame.draw_image(STANDARD_RECTANGLE, Image::new(tile_and_pos.image.clone()));
                })
            }
        });

        vec![content]
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
