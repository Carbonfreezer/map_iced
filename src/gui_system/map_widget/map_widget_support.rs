//! This module contains various functions related to `map_widget`, that are taken out here
//! to make the module slimmer. These are the text functions and the annotation interaction functions.

use crate::annotation_system::annotation_support::AnnotationKey;
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use crate::gui_system::map_widget::map_widget_components::{
    MapInteractionCommand, SpecificInteractionCommand,
};
use iced::advanced::graphics::geometry::Frame;
use iced::widget::Action;
use iced::widget::canvas::Text;
use iced::{Color, Renderer, Vector};
use std::time::{Duration, Instant};

/// The light outline behind the map texts and the scale bar. No single colour reads
/// on every map style, the outline keeps them readable on dark ones such as
/// satellite imagery.
pub(crate) const HALO_COLOR: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.8);

/// How far the text halo reaches around each glyph, in pixels.
const HALO_WIDTH: f32 = 1.0;

/// How long the cursor has to rest on an annotation before its description shows up.
const HOVER_DELAY: Duration = Duration::from_secs(1);

/// Draws `text` with a halo in [`HALO_COLOR`]: first in the halo colour, shifted to
/// the eight neighbouring positions, then once in its own colour on top. Every
/// overlay text goes through here.
pub(crate) fn fill_text_with_halo(frame: &mut Frame<Renderer>, text: Text) {
    for (dx, dy) in [
        (-1.0, -1.0),
        (0.0, -1.0),
        (1.0, -1.0),
        (-1.0, 0.0),
        (1.0, 0.0),
        (-1.0, 1.0),
        (0.0, 1.0),
        (1.0, 1.0),
    ] {
        frame.fill_text(Text {
            position: text.position + Vector::new(dx, dy) * HALO_WIDTH,
            color: HALO_COLOR,
            ..text.clone()
        });
    }
    frame.fill_text(text);
}

/// What the cursor points at: an annotation together with the spot its description
/// is anchored to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HoverTarget {
    /// The hovered annotation, see [`MapWidget::annotation_at`](super::map_widget_core::MapWidget::annotation_at). Unlike an index into
    /// the snapshot, the key stays valid when the snapshot is replaced.
    pub(crate) key: AnnotationKey,
    /// Where the description goes, resolved from [`Annotation::label_anchor`](crate::annotation_system::annotation_support::Annotation::label_anchor) when the
    /// cursor came to rest.
    ///
    /// Part of the target on purpose: a fixed anchor, as for a way point, stays the
    /// same while the cursor moves over the symbol, one that follows the cursor, as
    /// for a region, changes with every move and so restarts the dwell timer.
    pub(crate) anchor: LatitudeLongitude,
}

/// The result of a hit test under the cursor: the [`HoverTarget`] together with the
/// cursor position it was found at. The cursor position is kept out of the target,
/// otherwise every move would count as a new target and restart the dwell timer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HoverHit {
    /// What the cursor points at.
    pub(crate) target: HoverTarget,
    /// Where the cursor is, handed to the application with the announcement.
    pub(crate) cursor: LatitudeLongitude,
}

/// A [`HoverTarget`] together with the moment its description is due.
#[derive(Debug, Clone, Copy)]
struct Hover {
    /// What the cursor rests on.
    target: HoverTarget,
    /// The instant from which on the description is shown.
    visible_at: Instant,
}

/// The interaction state of the [`AnnotationOverlay`](super::map_widget_components::AnnotationOverlay). Separate from
/// [`InteractionState`](super::map_widget_components::InteractionState), because the overlay is a canvas of its own and therefore
/// carries its own widget state.
#[derive(Debug, Default)]
pub(crate) struct AnnotationInteractionState {
    /// The annotation under the cursor, if any.
    hovered: Option<Hover>,
}

impl AnnotationInteractionState {
    /// Points the hover at the target of `hit`.
    ///
    /// The dwell timer only restarts when the target actually changes, so the jitter
    /// of a cursor resting inside a way point symbol does not keep pushing the
    /// description away. A description that follows the cursor changes its target
    /// with every move, so it hides while the cursor moves and shows up again where
    /// the cursor comes to rest.
    ///
    /// A new target gets announced to the client `client_id`, so the application can
    /// adapt the description during the dwell time.
    pub(crate) fn aim(
        &mut self,
        hit: Option<HoverHit>,
        client_id: u32,
    ) -> Option<Action<MapInteractionCommand>> {
        let target = hit.map(|hit| hit.target);
        if self.hovered.map(|hover| hover.target) == target {
            return None;
        }

        let was_visible = self.description_target().is_some();
        self.hovered = target.map(|target| Hover {
            target,
            visible_at: Instant::now() + HOVER_DELAY,
        });

        match hit {
            // An action either publishes or asks for a frame, not both. Publishing is
            // enough: iced redraws after the message, which takes a visible
            // description away, and that frame reaches settle, which asks for the
            // frame at visible_at. Sequence is here: We store the timer, publish AnnotationHovered
            // publishing any message evokes in the iced system a redraw event (see doc Action::publish),
            // that invokes settle, that requests a redraw at some point in the future.
            Some(hit) => Some(Action::publish(MapInteractionCommand {
                client_id,
                command: SpecificInteractionCommand::AnnotationHovered(hit.target.key, hit.cursor),
            })),
            // Nothing to wait for, but a description that is on screen has to go.
            None => was_visible.then(Action::request_redraw),
        }
    }

    /// Handles the frame that was requested by [`Self::aim`]. Re-arms the request if
    /// the frame arrived before the dwell time was actually over.
    pub(crate) fn settle(&self, now: Instant) -> Option<Action<MapInteractionCommand>> {
        let hover = self.hovered?;
        (now < hover.visible_at).then(|| Action::request_redraw_at(hover.visible_at))
    }

    /// The target whose description is due by now, if any.
    pub(crate) fn description_target(&self) -> Option<HoverTarget> {
        self.hovered
            .filter(|hover| Instant::now() >= hover.visible_at)
            .map(|hover| hover.target)
    }
}
