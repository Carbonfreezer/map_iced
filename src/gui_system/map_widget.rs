//! This contains the core map widget.

use crate::annotation_system::waypoint_system::{WaypointInfo, WaypointKey, InternalWaypointImage};
use crate::gui_system::direction_arrow::{
    PlacedArrow, attached_arrow, draw_arrow, radar_arrow, symbol_visible,
};
use crate::gui_system::focus_animation::FocusAnimation;
use crate::gui_system::high_level_tile_cache::TilesToDraw;
use crate::gui_system::internal_math::{
    BoundingRectangle, DrawingPositionConverter, LatitudeLongitude, MAXIMUM_ZOOM_LEVEL,
    RectConversionError, TILE_SIZE_PIXEL,
};
use iced::advanced::graphics::geometry::Frame;
use iced::advanced::image::Image;
use iced::mouse::{Cursor, Interaction, ScrollDelta};
use iced::time::{Duration, Instant};
use iced::widget::canvas::{Cache, Geometry, Path, Stroke, Text, stroke};
use iced::widget::{Action, canvas};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, mouse, window};

/// The velocity we use for mouse scrolling.
const SCROLLING_SPEED: f32 = 0.05;

/// The font size we want to use.
const FONT_SIZE: f32 = 15.0;

/// The color we use for drawing overlay text.
const TEXT_COLOR: Color = Color::from_rgb(0.6, 0.4, 0.4);

/// Half the size of the way point we apply.
const WAYPOINT_HALF_SIZE: f32 = 10.0;

/// Horizontal gap between a way point symbol and its hover description.
const DESCRIPTION_GAP: f32 = 4.0;

/// How long the cursor has to rest on a way point before its description shows up.
const HOVER_DELAY: Duration = Duration::from_secs(1);

/// These become the interaction commands with the rest of the system later on. These
/// commands contain the information of a specific client widget.
#[derive(Debug, Clone)]
pub struct MapInteractionCommand {
    pub(crate) client_id: u32,
    pub(crate) command: SpecificInteractionCommand,
}

/// Focal point info consisting of latitude, longitude and a
/// continuous zoom level.
#[derive(Debug, Clone, Copy)]
pub struct FocalPoint {
    pub position: LatitudeLongitude,
    pub continuous_zoom_level: f32,
}

/// These are the interaction commands for a specific client widget. The association with the
/// client widget is given over [`MapInteractionCommand`]. They report what happened in
/// the widget; what it means is decided by [`MapWidgetSystem`].
///
/// [`MapWidgetSystem`]: crate::gui_system::map_widget_system::MapWidgetSystem
#[derive(Debug, Clone)]
pub enum SpecificInteractionCommand {
    /// We want to set the focal point as latitude longitude and the zoom level.
    SetFocalPoint(FocalPoint, Rectangle),
    /// A left click landed on the given way point.
    WaypointClicked(WaypointKey),
    /// One frame of the soft focus started under `generation`. `finished` marks the
    /// last frame, which sits exactly on the target.
    AnimationFrame {
        focal_point: FocalPoint,
        bounds: Rectangle,
        generation: u64,
        finished: bool,
    },
}

/// The internal state for mouse processing.
#[derive(Default)]
pub struct InteractionState {
    /// Contains the last position, when the middle mouse button is pressed.
    drag_origin: Option<Point>,
}

/// The widget used for rendering a tile.
pub struct MapWidget {
    /// The drawing cache for the tiles.
    tile_drawing_cache: Cache,
    /// The copyright text overlay.
    overlay_cache: Cache,
    /// The drawing cache for the waypoint info.
    waypoint_cache: Cache,
    /// Tiles for the current view, possibly still filling up.
    drawing_tiles: Vec<TilesToDraw>,
    /// Last complete set, kept as backdrop while the current one fills up.
    fallback_tiles: Vec<TilesToDraw>,
    /// The internal id of the widget.
    client_id: u32,
    /// The view this widget currently shows. Source of truth for interaction.
    focal_point: FocalPoint,
    /// Derived from `focal_point` plus the canvas bounds, for rendering only.
    position_converter: Option<DrawingPositionConverter>,
    /// The copyright text we need for drawing.
    copyright_text: String,
    /// Way point info ix existing.
    waypoint_info: Vec<WaypointInfo>,
    /// The direction arrows of the current view, see [`Self::place_arrows`].
    arrows: Vec<PlacedArrow>,
    /// Flags that we want to have a focal reset usually because of waypoint or annotation changes from the outside.
    request_focal_reset: bool,
    /// The soft focus currently running, if any.
    animation: Option<FocusAnimation>,
    /// Counts the animations started, so that a frame published for an animation
    /// that has been replaced or cancelled in the meantime can be recognised.
    animation_generation: u64,
}

/// The rectangle that covers one tile.
const STANDARD_RECTANGLE: Rectangle = Rectangle {
    x: 0.0,
    y: 0.0,
    width: TILE_SIZE_PIXEL as f32,
    height: TILE_SIZE_PIXEL as f32,
};

impl MapWidget {
    /// Creates a new widget from the client id.
    pub fn new(client_id: u32, copyright_text: String, focal_point: FocalPoint) -> Self {
        Self {
            tile_drawing_cache: Default::default(),
            overlay_cache: Default::default(),
            waypoint_cache: Default::default(),
            drawing_tiles: vec![],
            fallback_tiles: vec![],
            client_id,
            position_converter: None,
            focal_point,
            copyright_text,
            waypoint_info: vec![],
            arrows: vec![],
            request_focal_reset: true,
            animation: None,
            animation_generation: 0,
        }
    }

    /// The view this widget currently shows. During a soft focus this is the frame
    /// shown last.
    pub(crate) fn focal_point(&self) -> FocalPoint {
        self.focal_point
    }

    /// The hard focus: jumps to `focal_point` and cancels a running soft focus. The
    /// new view is applied with the next event, the widget needs its bounds for it.
    pub(crate) fn set_focal_point(&mut self, focal_point: FocalPoint) {
        self.cancel_animation();
        self.focal_point = focal_point;
        self.request_focal_reset = true;
    }

    /// The soft focus, see [`FocusAnimation`] for the path it takes. A widget that
    /// has never been laid out has no size to plan with, it jumps instead.
    pub(crate) fn animate_to(&mut self, target: LatitudeLongitude) {
        let Some(view_size) = self.position_converter.as_ref().map(|c| c.drawing_size()) else {
            self.set_focal_point(FocalPoint {
                position: target,
                ..self.focal_point
            });
            return;
        };
        self.animation_generation += 1;
        self.animation = Some(FocusAnimation::new(
            self.focal_point,
            target,
            view_size,
            Instant::now(),
        ));
    }

    /// Whether a frame published under `generation` still belongs to the running
    /// animation.
    pub(crate) fn is_current_animation(&self, generation: u64) -> bool {
        self.animation.is_some() && self.animation_generation == generation
    }

    /// Drops the running animation. Frames already published for it are ignored.
    pub(crate) fn cancel_animation(&mut self) {
        self.animation = None;
    }

    /// Rebuilds the converter for a new view and reports the tiles it needs.
    pub fn apply_focal_point(
        &mut self,
        focal_point: FocalPoint,
        bounds: Rectangle,
    ) -> Option<BoundingRectangle> {
        self.request_focal_reset = false;
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
        // TODO: Here we have to ask the map widget system for way point information. and also invalidate the overlay cache.
        self.tile_drawing_cache.clear();
        debug_assert!(
            !matches!(rectangle, Err(RectConversionError::NegativeSize)),
            "Negative size in rectangle detected."
        );
        rectangle.ok()
    }

    /// Does an invalidation by requesting a reset of the focal point. This is
    /// relevant, if waypoints change.
    pub(crate) fn request_focal_reset(&mut self) {
        self.request_focal_reset = true;
    }

    /// Called from outside the map widget system to set the waypoint information:
    /// the way points around the view, and all flagged ones for the arrows.
    pub(crate) fn set_waypoint_info(
        &mut self,
        way_points: Vec<WaypointInfo>,
        flagged: Vec<WaypointInfo>,
    ) {
        self.waypoint_info = way_points;
        self.place_arrows(&flagged);
        self.waypoint_cache.clear();
    }

    /// Places the direction arrows for the current view.
    ///
    /// An off screen way point gets a radar arrow on the edge. One that comes into
    /// view during a soft focus keeps the arrow it had in the frame before, now
    /// attached to its symbol and with the direction frozen, until the animation is
    /// over. Otherwise the arrow would vanish halfway through the move, and taking
    /// the direction from the centre would spin it once the centre reaches the target.
    fn place_arrows(&mut self, flagged: &[WaypointInfo]) {
        let Some(converter) = &self.position_converter else {
            self.arrows.clear();
            return;
        };
        let size = converter.drawing_size();
        let animating = self.animation.is_some();
        let previous = &self.arrows;

        let arrows = flagged
            .iter()
            .filter_map(|point| {
                let color = point.flag?;
                let target = converter.get_unclipped_drawing_position(point.position);
                let placement = if !symbol_visible(size, target, WAYPOINT_HALF_SIZE as f64) {
                    radar_arrow(size, target)?
                } else if animating {
                    let carried = previous.iter().find(|arrow| arrow.key == point.key)?;
                    attached_arrow(target, carried.placement.direction, WAYPOINT_HALF_SIZE)
                } else {
                    return None;
                };
                Some(PlacedArrow {
                    key: point.key,
                    placement,
                    color,
                })
            })
            .collect();
        self.arrows = arrows;
    }

    /// Called from the outside if new tiles have arrived.
    pub(crate) fn set_drawing_tiles(&mut self, drawing_tiles: Vec<TilesToDraw>) {
        self.drawing_tiles = drawing_tiles;
        self.tile_drawing_cache.clear();
    }

    fn publish(
        &self,
        focal_point: FocalPoint,
        bounds: Rectangle,
    ) -> Option<Action<MapInteractionCommand>> {
        Some(Action::publish(MapInteractionCommand {
            client_id: self.client_id,
            command: SpecificInteractionCommand::SetFocalPoint(focal_point, bounds),
        }))
    }

    /// Prints the copyright information into the lower right corner
    fn print_copyright_text(&self, bounds: Rectangle, frame: &mut Frame<Renderer>) {
        // 1. Calculate the bounding box of your text (needed to offset the position)
        let text_content = self.copyright_text.clone();

        // 2. Measure or estimate the dimensions
        // Iced uses an approximate width based on character count if not measured directly
        let estimated_width = text_content.len() as f32 * (FONT_SIZE * 0.5); // Rough estimate

        // 3. Subtract the text size from the canvas bounds
        let padding = 5.0; // Distance from the absolute edges
        let x = bounds.width - estimated_width - padding;
        let y = bounds.height - FONT_SIZE - padding;

        // 4. Draw the text
        frame.fill_text(Text {
            content: text_content,
            position: Point::new(x, y),
            color: TEXT_COLOR,
            size: FONT_SIZE.into(),
            ..Default::default()
        });
    }

    /// Hit test in widget coordinates. Returns the index of the topmost way point
    /// of the current snapshot that covers `position`.
    ///
    /// The index refers to the snapshot installed by [`Self::set_waypoint_info`],
    /// which is replaced on every focal point change. Resolve it within the event
    /// that produced `position` rather than storing it across frames.
    pub(crate) fn waypoint_at(&self, position: Point) -> Option<usize> {
        let converter = self.position_converter.as_ref()?;
        self.waypoint_info
            .iter()
            .enumerate()
            .rev()
            .find(|(_, annotation)| {
                converter
                    .get_drawing_position(annotation.position, WAYPOINT_HALF_SIZE as f64)
                    .is_some_and(|symbol| {
                        ((position.x - symbol.x).abs() <= WAYPOINT_HALF_SIZE)
                            && ((position.y - symbol.y).abs() <= WAYPOINT_HALF_SIZE)
                    })
            })
            .map(|(index, _)| index)
    }

    /// A way point of the current snapshot. See [`Self::waypoint_at`] for how long
    /// an index stays valid.
    pub(crate) fn waypoint(&self, index: usize) -> Option<&WaypointInfo> {
        self.waypoint_info.get(index)
    }

    /// The way point symbols. Cached, because they only change with the way point
    /// snapshot or the focal point, never with the cursor.
    fn draw_waypoint_symbols(&self, renderer: &Renderer, bounds: Rectangle) -> Geometry<Renderer> {
        self.waypoint_cache.draw(renderer, bounds.size(), |frame| {
            let Some(converter) = &self.position_converter else {
                return;
            };
            for annotation in &self.waypoint_info {
                let Some(draw_pos) =
                    converter.get_drawing_position(annotation.position, WAYPOINT_HALF_SIZE as f64)
                else {
                    continue;
                };
                match &annotation.image {
                    InternalWaypointImage::Image(handle) => {
                        // Centred on the position, with the same extent as the cross.
                        frame.draw_image(
                            Rectangle {
                                x: draw_pos.x - WAYPOINT_HALF_SIZE,
                                y: draw_pos.y - WAYPOINT_HALF_SIZE,
                                width: WAYPOINT_HALF_SIZE * 2.0,
                                height: WAYPOINT_HALF_SIZE * 2.0,
                            },
                            Image::new(handle.clone()),
                        );
                    }
                    InternalWaypointImage::Cross(color) => {
                        let line_stroke = Stroke {
                            width: 2.0,
                            style: stroke::Style::Solid(*color),
                            ..Stroke::default()
                        };

                        frame.stroke(
                            &Path::line(
                                Point::new(-WAYPOINT_HALF_SIZE, -WAYPOINT_HALF_SIZE) + draw_pos,
                                Point::new(WAYPOINT_HALF_SIZE, WAYPOINT_HALF_SIZE) + draw_pos,
                            ),
                            line_stroke,
                        );

                        frame.stroke(
                            &Path::line(
                                Point::new(-WAYPOINT_HALF_SIZE, WAYPOINT_HALF_SIZE) + draw_pos,
                                Point::new(WAYPOINT_HALF_SIZE, -WAYPOINT_HALF_SIZE) + draw_pos,
                            ),
                            line_stroke,
                        );
                    }
                }
            }

            for arrow in &self.arrows {
                draw_arrow(frame, arrow.placement, arrow.color);
            }
        })
    }

    /// Everything that depends on the overlay interaction state. Deliberately *not*
    /// cached, so a changed state shows up on the next redraw without anyone having
    /// to invalidate a cache.
    fn draw_annotation_interaction(
        &self,
        renderer: &Renderer,
        bounds: Rectangle,
        state: &AnnotationInteractionState,
    ) -> Geometry<Renderer> {
        let mut frame = Frame::new(renderer, bounds.size());

        if let (Some(annotation), Some(converter)) = (
            state
                .description_index()
                .and_then(|index| self.waypoint(index)),
            self.position_converter.as_ref(),
        )
            && let (Some(description), Some(anchor)) = (
                annotation.description.as_ref(),
                converter.get_drawing_position(annotation.position, WAYPOINT_HALF_SIZE as f64),
            ) {
                frame.fill_text(Text {
                    content: description.clone(),
                    position: Point::new(anchor.x + WAYPOINT_HALF_SIZE + DESCRIPTION_GAP, anchor.y),
                    color: TEXT_COLOR,
                    size: FONT_SIZE.into(),
                    ..Default::default()
                });
            }

        frame.into_geometry()
    }

    /// The copyright overlay. Cached, it only depends on the widget size.
    fn draw_copyright(&self, renderer: &Renderer, bounds: Rectangle) -> Geometry<Renderer> {
        self.overlay_cache.draw(renderer, bounds.size(), |frame| {
            self.print_copyright_text(bounds, frame);
        })
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
        if self.request_focal_reset {
            return self.publish(self.focal_point, bounds);
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

        match event {
            Event::Window(window::Event::Resized(_)) => self.publish(self.focal_point, bounds),

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
        // The cursor shape for the way points is decided here, in the lower canvas
        // of the stack, and not in the overlay on top of it. `stack` levitates the
        // cursor for every child below as soon as an upper child claims an
        // interaction, which would cut this widget off from the middle button and
        // the wheel. Nothing sits below this one, so claiming here is free.
        if state.drag_origin.is_some() {
            return Interaction::Grabbing;
        }

        match cursor
            .position_in(bounds)
            .and_then(|position| self.waypoint_at(position))
        {
            Some(_) => Interaction::Pointer,
            None => Interaction::None,
        }
    }
}

/// A way point the cursor currently rests on, together with the moment its
/// description is due.
#[derive(Debug, Clone, Copy)]
struct Hover {
    /// Index into the current way point snapshot, see [`MapWidget::waypoint_at`].
    index: usize,
    /// The instant from which on the description is shown.
    visible_at: Instant,
}

/// The interaction state of the [`AnnotationOverlay`]. Separate from
/// [`InteractionState`], because the overlay is a canvas of its own and therefore
/// carries its own widget state.
#[derive(Debug, Default)]
pub struct AnnotationInteractionState {
    /// The way point under the cursor, if any.
    hovered: Option<Hover>,
}

impl AnnotationInteractionState {
    /// Points the hover at `index`.
    ///
    /// The dwell timer only restarts when the target actually changes, so the jitter
    /// of a cursor resting inside one symbol does not keep pushing the description
    /// away.
    fn aim(&mut self, index: Option<usize>) -> Option<Action<MapInteractionCommand>> {
        if self.hovered.map(|hover| hover.index) == index {
            return None;
        }

        let was_visible = self.description_index().is_some();
        self.hovered = index.map(|index| Hover {
            index,
            visible_at: Instant::now() + HOVER_DELAY,
        });

        match self.hovered {
            // Ask to be woken up when the dwell time is over.
            Some(hover) => Some(Action::request_redraw_at(hover.visible_at)),
            // Nothing to wait for, but a description that is on screen has to go.
            None if was_visible => Some(Action::request_redraw()),
            None => None,
        }
    }

    /// Handles the frame that was requested by [`Self::aim`]. Re-arms the request if
    /// the frame arrived before the dwell time was actually over.
    fn settle(&self, now: Instant) -> Option<Action<MapInteractionCommand>> {
        let hover = self.hovered?;
        (now < hover.visible_at).then(|| Action::request_redraw_at(hover.visible_at))
    }

    /// The way point whose description is due by now, if any.
    fn description_index(&self) -> Option<usize> {
        self.hovered
            .filter(|hover| Instant::now() >= hover.visible_at)
            .map(|hover| hover.index)
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
pub struct AnnotationOverlay<'a> {
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
            Event::Mouse(mouse::Event::CursorMoved { .. }) => state.aim(
                cursor
                    .position_in(bounds)
                    .and_then(|position| self.widget.waypoint_at(position)),
            ),

            Event::Mouse(mouse::Event::CursorLeft) => state.aim(None),

            Event::Window(window::Event::RedrawRequested(now)) => state.settle(*now),

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                let hit = self
                    .widget
                    .waypoint_at(position)
                    .and_then(|index| self.widget.waypoint(index))?
                    .key;

                // A click that hits nothing is not our business, it stays available to
                // the map below. Capturing here keeps a hit click from also reaching it.
                Some(
                    Action::publish(MapInteractionCommand {
                        client_id: self.widget.client_id,
                        command: SpecificInteractionCommand::WaypointClicked(hit),
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
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        vec![
            self.widget.draw_waypoint_symbols(renderer, bounds),
            self.widget
                .draw_annotation_interaction(renderer, bounds, state),
            self.widget.draw_copyright(renderer, bounds),
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
        // the cursor sits on a way point. The way point cursor shape therefore lives
        // in `MapWidget::mouse_interaction`, and a click is claimed by capturing the
        // event in `update`, which only consumes that single event.
        Interaction::None
    }
}
