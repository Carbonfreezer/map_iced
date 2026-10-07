//! This module contains the core par of map widget.

use crate::annotation_system::waypoint_system::{InternalWaypointImage, WaypointInfo, WaypointKey};
use crate::gui_system::high_level_tile_cache::TilesToDraw;
use crate::gui_system::internal_math::{
    BoundingRectangle, DrawingPositionConverter, RectConversionError, TILE_SIZE_PIXEL,
};
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::direction_arrow::{
    PlacedArrow, arrow_hit, attached_arrow, draw_arrow, radar_arrow, symbol_visible,
};
use crate::gui_system::map_widget::focus_animation::FocusAnimation;
use crate::gui_system::map_widget::map_widget_components::{
    FONT_SIZE, FocalPoint, MapInteractionCommand, SpecificInteractionCommand, TEXT_COLOR,
};
use crate::gui_system::map_widget::map_widget_support::{
    AnnotationInteractionState, fill_text_with_halo,
};
use crate::gui_system::map_widget::scale_bar::{draw_scale_bar, scale_bar};
use iced::advanced::graphics::geometry::Frame;
use iced::advanced::image::Image;
use iced::widget::Action;
use iced::widget::canvas::{Cache, Geometry, Path, Stroke, Text, stroke};
use iced::{Point, Rectangle, Renderer, Vector};
use std::time::Instant;

/// Half the size of the way point we apply.
const WAYPOINT_HALF_SIZE: f32 = 15.0;

/// Horizontal gap between a way point symbol and its hover description.
const DESCRIPTION_GAP: f32 = 4.0;

/// The widget used for rendering a tile.
pub(crate) struct MapWidget {
    /// The drawing cache for the tiles.
    pub(crate) tile_drawing_cache: Cache,
    /// The copyright text overlay.
    overlay_cache: Cache,
    /// The drawing cache for the waypoint info.
    waypoint_cache: Cache,
    /// The drawing cache for the scale bar, it changes with the focal point.
    scale_cache: Cache,
    /// Tiles for the current view, possibly still filling up.
    pub(crate) drawing_tiles: Vec<TilesToDraw>,
    /// Last complete set, kept as backdrop while the current one fills up.
    pub(crate) fallback_tiles: Vec<TilesToDraw>,
    /// The internal id of the widget.
    pub(crate) client_id: u32,
    /// The view this widget currently shows. Source of truth for interaction.
    pub(crate) focal_point: FocalPoint,
    /// Derived from `focal_point` plus the canvas bounds, for rendering only.
    pub(crate) position_converter: Option<DrawingPositionConverter>,
    /// The copyright text we need for drawing.
    copyright_text: String,
    // TODO: Replace vec waypoint info with placeable info that uses internal keys only.
    /// Way point info ix existing.
    waypoint_info: Vec<WaypointInfo>,
    /// The direction arrows of the current view, see [`Self::place_arrows`].
    arrows: Vec<PlacedArrow>,
    /// Flags that we want to have a focal reset usually because of waypoint or annotation changes from the outside.
    pub(crate) request_focal_reset: bool,
    /// The soft focus currently running, if any.
    pub(crate) animation: Option<FocusAnimation>,
    /// Counts the animations started, so that a frame published for an animation
    /// that has been replaced or cancelled in the meantime can be recognised.
    pub(crate) animation_generation: u64,
}

/// The rectangle that covers one tile.
pub(crate) const STANDARD_RECTANGLE: Rectangle = Rectangle {
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
            scale_cache: Default::default(),
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
        self.tile_drawing_cache.clear();
        self.scale_cache.clear();
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

    
    // TODO: The information if flagged should get inside the waypoint.
    
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

        let centre = Vector::new(size.width as f64 * 0.5, size.height as f64 * 0.5);
        let mut ranked: Vec<_> = flagged
            .iter()
            .filter_map(|point| {
                let flag = point.flag?;
                let target = converter.get_unclipped_drawing_position(point.position);
                let distance = (target.x - centre.x).hypot(target.y - centre.y);
                let placement = if !symbol_visible(size, target, WAYPOINT_HALF_SIZE as f64) {
                    radar_arrow(size, target)?
                } else if animating {
                    let carried = previous.iter().find(|arrow| arrow.key == point.key)?;
                    attached_arrow(target, carried.placement.direction, WAYPOINT_HALF_SIZE)
                } else {
                    return None;
                };
                let arrow = PlacedArrow {
                    key: point.key,
                    placement,
                    color: flag.color,
                };
                Some((flag.priority, distance, arrow))
            })
            .collect();

        // Drawn in this order, so the last one lies on top and wins the click: the
        // highest priority, and within one priority the nearest way point, which is
        // the one to head for first.
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
        self.arrows = ranked.into_iter().map(|(_, _, arrow)| arrow).collect();
    }

    /// Called from the outside if new tiles have arrived.
    pub(crate) fn set_drawing_tiles(&mut self, drawing_tiles: Vec<TilesToDraw>) {
        self.drawing_tiles = drawing_tiles;
        self.tile_drawing_cache.clear();
    }

    /// Helper function to issue a command to set a focal point.
    pub(crate) fn publish(
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
        fill_text_with_halo(
            frame,
            Text {
                content: text_content,
                position: Point::new(x, y),
                color: TEXT_COLOR,
                size: FONT_SIZE.into(),
                ..Default::default()
            },
        );
    }

    /// Hit test in widget coordinates against the direction arrows. Returns the key
    /// of the way point whose arrow covers `position`, the topmost one first.
    pub(crate) fn arrow_at(&self, position: Point) -> Option<WaypointKey> {
        self.arrows
            .iter()
            .rev()
            .find(|arrow| arrow_hit(arrow.placement, position))
            .map(|arrow| arrow.key)
    }

    // TODO: replace with annotation info that gets a general key to placeable.
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
    pub(crate) fn draw_waypoint_symbols(
        &self,
        renderer: &Renderer,
        bounds: Rectangle,
    ) -> Geometry<Renderer> {
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
    pub(crate) fn draw_annotation_interaction(
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
        ) && let (Some(description), Some(anchor)) = (
            annotation.description.as_ref(),
            converter.get_drawing_position(annotation.position, WAYPOINT_HALF_SIZE as f64),
        ) {
            fill_text_with_halo(
                &mut frame,
                Text {
                    content: description.clone(),
                    position: Point::new(anchor.x + WAYPOINT_HALF_SIZE + DESCRIPTION_GAP, anchor.y),
                    color: TEXT_COLOR,
                    size: FONT_SIZE.into(),
                    ..Default::default()
                },
            );
        }

        frame.into_geometry()
    }

    /// The copyright overlay. Cached, it only depends on the widget size.
    pub(crate) fn draw_copyright(
        &self,
        renderer: &Renderer,
        bounds: Rectangle,
    ) -> Geometry<Renderer> {
        self.overlay_cache.draw(renderer, bounds.size(), |frame| {
            self.print_copyright_text(bounds, frame);
        })
    }

    /// The scale bar, measured at the latitude of the centre of the view.
    pub(crate) fn draw_scale(&self, renderer: &Renderer, bounds: Rectangle) -> Geometry<Renderer> {
        self.scale_cache.draw(renderer, bounds.size(), |frame| {
            if let Some(bar) = scale_bar(
                self.focal_point.position.latitude,
                self.focal_point.continuous_zoom_level,
            ) {
                draw_scale_bar(frame, bounds.size(), &bar);
            }
        })
    }
}
