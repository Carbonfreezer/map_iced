//! This module contains various functions related to `map_widget`, that are taken out here
//! to make the module slimmer. These are the text functions and the annotation interaction functions.

use crate::gui_system::map_widget_components::MapInteractionCommand;
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

/// How long the cursor has to rest on a way point before its description shows up.
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
pub(crate) struct AnnotationInteractionState {
    /// The way point under the cursor, if any.
    hovered: Option<Hover>,
}

impl AnnotationInteractionState {
    /// Points the hover at `index`.
    ///
    /// The dwell timer only restarts when the target actually changes, so the jitter
    /// of a cursor resting inside one symbol does not keep pushing the description
    /// away.
    pub(crate) fn aim(&mut self, index: Option<usize>) -> Option<Action<MapInteractionCommand>> {
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
    pub(crate) fn settle(&self, now: Instant) -> Option<Action<MapInteractionCommand>> {
        let hover = self.hovered?;
        (now < hover.visible_at).then(|| Action::request_redraw_at(hover.visible_at))
    }

    /// The way point whose description is due by now, if any.
    pub(crate) fn description_index(&self) -> Option<usize> {
        self.hovered
            .filter(|hover| Instant::now() >= hover.visible_at)
            .map(|hover| hover.index)
    }
}
