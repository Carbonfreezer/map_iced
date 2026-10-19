//! This contains the map widget components which are mainly the canvasses.

use crate::annotation_system::annotation_support::{AnnotationKey, MercatorRectangle};
use crate::gui_system::internal_math::MAXIMUM_ZOOM_LEVEL;
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::map_widget_core::{MapWidget, STANDARD_RECTANGLE};
use crate::gui_system::map_widget::map_widget_support::AnnotationInteractionState;
use crate::gui_system::map_widget_system::MapWidgetSystem;
use iced::advanced::image::Image;
use iced::mouse::{Cursor, Interaction, ScrollDelta};
use iced::widget::canvas::Geometry;
use iced::widget::{Action, canvas};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, mouse, window};

/// The velocity we use for mouse scrolling.
const SCROLLING_SPEED: f32 = 0.05;

/// The font size we want to use.
pub(crate) const FONT_SIZE: f32 = 15.0;

/// The color we use for drawing overlay text.
pub(crate) const TEXT_COLOR: Color = Color::BLACK;

/// The message a map widget produces, tagged with the widget it comes from. Public
/// because the application maps it into its own message type in its view and hands
/// it back to the [`MapWidgetSystem`](crate::gui_system::map_widget_system::MapWidgetSystem)
/// as a [`MapWidgetMessage`](crate::gui_system::map_widget_system::MapWidgetMessage).
///
/// # Example
/// ```
/// use map_iced::gui_system::map_widget_system::{MapWidgetSystem, MapWidgetMessage};
/// use iced::Element;
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     WidgetMessage(MapWidgetMessage),
///     InternalMessage(usize, bool),
/// }
///
/// fn get_map_element(widget_id: u32,  widget_system: &MapWidgetSystem) -> Element<'_, Message> {
///    widget_system
///             .canvas(widget_id)
///             .map(|cmd| Message::WidgetMessage(cmd.into()))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MapInteractionCommand {
    pub(crate) client_id: u32,
    pub(crate) command: SpecificInteractionCommand,
}

/// Focal point info consisting of latitude, longitude and a
/// continuous zoom level.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FocalPoint {
    pub(crate) position: LatitudeLongitude,
    pub(crate) continuous_zoom_level: f32,
}

/// What a focus aims at. Turned into a `FocalPoint` by the widget itself, since
/// only the widget knows its size at that moment.
#[derive(Debug, Clone)]
pub enum FocusTarget {
    /// A single position, the zoom level stays as it is.
    Position(LatitudeLongitude),
    /// An area, typically the bounds of a region. It gets centred and zoomed so that
    /// it covers the share `fill` (0..=1) of the widget's width or height, whichever
    /// is binding.
    Area {
        bounds: MercatorRectangle,
        fill: f32,
    },
}

impl From<LatitudeLongitude> for FocusTarget {
    fn from(position: LatitudeLongitude) -> Self {
        FocusTarget::Position(position)
    }
}

impl FocusTarget {
    /// The area within `radius_meter` around `center`, e.g. 200 m around a vehicle
    /// to keep in view. The circle covers the share
    /// [`MapWidgetSystem::DEFAULT_FOCUS_FILL`] of the widget's shorter side, so a bit
    /// more than the radius stays visible to every side.
    ///
    /// To follow a moving vehicle, call [`MapWidgetSystem::jump_to`] with each new
    /// position. [`MapWidgetSystem::animate_to`] starts its pan and zoom anew on every
    /// call, which suits single moves rather than a stream of them.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::LatitudeLongitude;
    /// use map_iced::gui_system::map_widget_system::{FocusTarget, MapWidgetSystem};
    /// # let cache = tempfile::tempdir().unwrap();
    /// # let config = map_iced::gui_system::tile_cache_construction::TileCacheConfig::new(
    /// #     map_iced::gui_system::tile_cache_construction::TileSource::TestPattern,
    /// #     map_iced::gui_system::tile_cache_construction::CachingDirectory::FullyConstructed(cache.path().into()),
    /// #     30_000,
    /// # );
    /// # let runtime = tokio::runtime::Runtime::new().unwrap();
    /// # let _guard = runtime.enter();
    /// let (mut system, _task) = MapWidgetSystem::boot(config).expect("tile cache set up");
    /// let widget = system.request_new_widget();
    /// // The latest position of the car, with 200 m of its surroundings.
    /// let car = LatitudeLongitude::new(49.7499, 6.6371);
    /// system.jump_to(widget, FocusTarget::around(car, 200.0));
    /// ```
    pub fn around(center: LatitudeLongitude, radius_meter: f64) -> Self {
        FocusTarget::Area {
            bounds: MercatorRectangle::create_around(center, radius_meter, 0.0),
            fill: MapWidgetSystem::DEFAULT_FOCUS_FILL,
        }
    }
}

/// These are the interaction commands for a specific client widget. The association with the
/// client widget is given over [`MapInteractionCommand`]. They report what happened in
/// the widget; what it means is decided by [`MapWidgetSystem`].
///
/// [`MapWidgetSystem`]: crate::gui_system::map_widget_system::MapWidgetSystem
#[derive(Debug, Clone)]
pub(crate) enum SpecificInteractionCommand {
    /// We want to set the focal point as latitude longitude and the zoom level.
    SetFocalPoint(FocalPoint, Rectangle),
    /// A left click landed on the given annotation element with the click point handed over as latitude, longitude.
    AnnotationClicked(AnnotationKey, LatitudeLongitude),
    /// The hover moved onto the given annotation element, with the cursor position
    /// handed over as latitude, longitude.
    AnnotationHovered(AnnotationKey, LatitudeLongitude),
    /// A left click landed on the direction arrow of the given annotation.
    ArrowClicked(AnnotationKey),
    /// One frame of the soft focus started under `generation`. `finished` marks the
    /// last frame, which sits exactly on the target.
    AnimationFrame {
        focal_point: FocalPoint,
        bounds: Rectangle,
        generation: u64,
        finished: bool,
    },
    /// The user has simply clicked onto a position on the map in latitude longitude.
    /// Gets superseded by a click on an arrow or an annotation.
    MapPointClicked(LatitudeLongitude),
}

/// The internal state for mouse processing.
#[derive(Default)]
pub(crate) struct InteractionState {
    /// Contains the last position, when the middle mouse button is pressed.
    drag_origin: Option<Point>,
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
        if self.request_focal_reset {
            return self.publish(self.focal_point_to_publish(bounds), bounds);
        }

        if let Some(animation) = &self.animation {
            return match event {
                // Every frame we publish is processed and followed by a redraw, so
                // the animation keeps itself running until its last frame.
                Event::Window(window::Event::RedrawRequested(now)) => {
                    let (focal_point, finished) = animation.sample(*now);
                    Some(Action::publish(MapInteractionCommand {
                        client_id: self.client_id,
                        command: SpecificInteractionCommand::AnimationFrame {
                            focal_point,
                            bounds,
                            generation: self.animation_generation,
                            finished,
                        },
                    }))
                }
                // Panning and zooming are suspended while the animation runs. A
                // drag in progress still follows the cursor, so that it does not
                // jump once the animation is over.
                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    if state.drag_origin.is_some() {
                        state.drag_origin = cursor.position_in(bounds);
                    }
                    None
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                    state.drag_origin = None;
                    None
                }
                _ => None,
            };
        }

        // Here we are not animating.
        // A resize is detected by comparing the bounds, not by `window::Event::Resized`:
        // iced delivers that event before the relayout, so it still carries the old
        // bounds. The `RedrawRequested` following the relayout brings the new ones.
        if self.view_size() != Some(bounds.size()) {
            return self.publish(self.focal_point_to_publish(bounds), bounds);
        }

        match event {
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: ScrollDelta::Lines { y, .. },
            }) if cursor.is_over(bounds) => {
                let zoom = (self.focal_point.continuous_zoom_level + y * SCROLLING_SPEED)
                    .clamp(0.0, MAXIMUM_ZOOM_LEVEL as f32);
                self.publish(
                    FocalPoint {
                        continuous_zoom_level: zoom,
                        ..self.focal_point
                    },
                    bounds,
                )
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

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                let converter = self.position_converter.as_ref()?;
                let conv_pos = converter.get_latitude_longitude_for_pixel_point(position);

                Some(Action::publish(MapInteractionCommand {
                    client_id: self.client_id,
                    command: SpecificInteractionCommand::MapPointClicked(conv_pos),
                }))
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
        let content = self
            .tile_drawing_cache
            .draw(renderer, bounds.size(), |frame| {
                for tile_and_pos in self.fallback_tiles.iter().chain(self.drawing_tiles.iter()) {
                    let Some(draw) = converter.get_draw_instruction(tile_and_pos.position.into())
                    else {
                        continue;
                    };
                    frame.with_save(|frame| {
                        frame.translate(draw.offset);
                        frame.scale(draw.scale);
                        // Snapped to the pixel grid, so that neighbouring tiles meet on
                        // whole pixels. Otherwise iced antialiases both edges and the
                        // background shows through the seam, flickering while animating.
                        frame.draw_image(
                            STANDARD_RECTANGLE,
                            Image::new(tile_and_pos.image.clone()).snap(true),
                        );
                    })
                }
            });

        vec![content]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Interaction {
        // The cursor shape for the annotations is decided here, in the lower canvas
        // of the stack, and not in the overlay on top of it. `stack` levitates the
        // cursor for every child below as soon as an upper child claims an
        // interaction, which would cut this widget off from the middle button and
        // the wheel. Nothing sits below this one, so claiming here is free.
        if state.drag_origin.is_some() {
            return Interaction::Grabbing;
        }

        let hovers_something = cursor.position_in(bounds).is_some_and(|position| {
            self.arrow_at(position).is_some() || self.annotation_at(position).is_some()
        });
        match hovers_something {
            true => Interaction::Pointer,
            false => Interaction::None,
        }
    }
}

/// Draws the annotations of a [`MapWidget`] into a canvas of its own.
///
/// All geometry returned by a single canvas ends up in one render layer, and inside
/// a layer iced renders strictly by primitive type - meshes, then images, then
/// text - and not in the order the geometry was returned. The way point symbols are
/// meshes and the map tiles are images, so a shared canvas would always bury the
/// symbols underneath the map. Stacking a second canvas on top gives the
/// annotations a layer, and hence a draw order, of their own.
pub(crate) struct AnnotationOverlay<'a> {
    widget: &'a MapWidget,
}

impl<'a> AnnotationOverlay<'a> {
    /// Wraps the widget whose annotations we draw.
    pub(crate) fn new(widget: &'a MapWidget) -> Self {
        Self { widget }
    }
}

impl canvas::Program<MapInteractionCommand> for AnnotationOverlay<'_> {
    type State = AnnotationInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<MapInteractionCommand>> {
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let position = cursor.position_in(bounds);
                state.aim(
                    position.and_then(|position| self.widget.hover_target_at(position)),
                    self.widget.client_id,
                )
            }

            Event::Mouse(mouse::Event::CursorLeft) => state.aim(None, self.widget.client_id),

            Event::Window(window::Event::RedrawRequested(now)) => state.settle(*now),

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                // The arrows are drawn on top of the symbols, so they win the hit.
                let command = match self.widget.arrow_at(position) {
                    Some(key) => SpecificInteractionCommand::ArrowClicked(key),
                    None => SpecificInteractionCommand::AnnotationClicked(
                        self.widget.annotation_at(position)?,
                        self.widget
                            .position_converter()?
                            .get_latitude_longitude_for_pixel_point(position),
                    ),
                };

                // A click that hits nothing is not our business, it stays available to
                // the map below. Capturing here keeps a hit click from also reaching it.
                Some(
                    Action::publish(MapInteractionCommand {
                        client_id: self.widget.client_id,
                        command,
                    })
                    .and_capture(),
                )
            }

            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        vec![
            self.widget.draw_annotation_symbols(renderer, bounds),
            self.widget.draw_annotation_interaction(
                renderer,
                bounds,
                state,
                cursor.position_in(bounds),
            ),
            self.widget.draw_copyright(renderer, bounds),
            self.widget.draw_scale(renderer, bounds),
        ]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Interaction {
        // Must stay `None`. Anything else makes `stack` levitate the cursor for the
        // children below, which would stop the map from panning and zooming while
        // the cursor sits on an annotation. The annotation cursor shape therefore lives
        // in `MapWidget::mouse_interaction`, and a click is claimed by capturing the
        // event in `update`, which only consumes that single event.
        Interaction::None
    }
}
