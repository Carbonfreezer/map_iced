//! The scale bar in the lower left corner. As on a paper map it holds for the
//! centre of the view: the Mercator scale grows with 1 / cos(latitude), so away from
//! the centre latitude the bar is only approximately right, and at low zoom levels,
//! where the view spans many degrees of latitude, not even that.

use crate::gui_system::internal_math::TILE_SIZE_PIXEL;
use crate::gui_system::map_widget::{FONT_SIZE, HALO_COLOR, TEXT_COLOR, fill_text_with_halo};
use iced::advanced::graphics::geometry::Frame;
use iced::alignment::Vertical;
use iced::widget::canvas::{Path, Stroke, Text, stroke};
use iced::{Color, Point, Renderer, Size};

/// The radius of the sphere Web Mercator is defined on, in metres.
const EARTH_RADIUS: f64 = 6_378_137.0;

/// The longest the bar may get, the actual length is the nicest round distance below.
const MAXIMUM_LENGTH: f32 = 120.0;

/// Distance of the bar from the left and the lower edge.
const MARGIN: f32 = 10.0;

/// Height of the end ticks.
const TICK_HEIGHT: f32 = 6.0;

/// Gap between the bar and its label.
const LABEL_GAP: f32 = 6.0;

/// The bar in the colour of the other map texts, on the same halo. Black turned out
/// to vanish on satellite imagery.
const BAR_COLOR: Color = TEXT_COLOR;

/// A scale bar ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScaleBar {
    /// Length on screen in pixels.
    pub length: f32,
    /// The distance it stands for, as text.
    pub label: String,
}

/// Ground distance per screen pixel at `latitude` (degrees) for the continuous
/// `zoom` level.
pub(crate) fn metres_per_pixel(latitude: f64, zoom: f32) -> f64 {
    let world_size = TILE_SIZE_PIXEL as f64 * f64::exp2(zoom as f64);
    2.0 * std::f64::consts::PI * EARTH_RADIUS * latitude.to_radians().cos() / world_size
}

/// The bar for a view centred at `latitude` with the continuous `zoom` level: the
/// longest distance of the form 1, 2 or 5 times a power of ten that fits into
/// [`MAXIMUM_LENGTH`] pixels.
pub(crate) fn scale_bar(latitude: f64, zoom: f32) -> Option<ScaleBar> {
    let per_pixel = metres_per_pixel(latitude, zoom);
    if !(per_pixel > 0.0) {
        return None;
    }
    let maximum = per_pixel * MAXIMUM_LENGTH as f64;
    let magnitude = f64::powi(10.0, maximum.log10().floor() as i32);
    let distance = [5.0, 2.0, 1.0]
        .into_iter()
        .map(|factor| factor * magnitude)
        .find(|&distance| distance <= maximum)?;

    let label = if distance >= 1000.0 {
        format!("{} km", distance / 1000.0)
    } else {
        format!("{distance} m")
    };
    Some(ScaleBar {
        length: (distance / per_pixel) as f32,
        label,
    })
}

/// Draws the bar into the lower left corner of a frame of `size`.
pub(crate) fn draw_scale_bar(frame: &mut Frame<Renderer>, size: Size, bar: &ScaleBar) {
    let left = MARGIN;
    let right = MARGIN + bar.length;
    let bottom = size.height - MARGIN;
    let top = bottom - TICK_HEIGHT;

    let shape = Path::new(|builder| {
        builder.move_to(Point::new(left, top));
        builder.line_to(Point::new(left, bottom));
        builder.line_to(Point::new(right, bottom));
        builder.line_to(Point::new(right, top));
    });
    for (width, color) in [(4.0, HALO_COLOR), (1.5, BAR_COLOR)] {
        frame.stroke(
            &shape,
            Stroke {
                width,
                style: stroke::Style::Solid(color),
                ..Stroke::default()
            },
        );
    }

    fill_text_with_halo(
        frame,
        Text {
            content: bar.label.clone(),
            position: Point::new(right + LABEL_GAP, bottom - TICK_HEIGHT * 0.5),
            color: BAR_COLOR,
            size: FONT_SIZE.into(),
            align_y: Vertical::Center,
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equator_at_zoom_zero_is_the_whole_earth() {
        let per_pixel = metres_per_pixel(0.0, 0.0);
        let circumference = 2.0 * std::f64::consts::PI * EARTH_RADIUS;
        assert!((per_pixel * TILE_SIZE_PIXEL as f64 - circumference).abs() < 1e-3);
    }

    #[test]
    fn scale_shrinks_with_latitude() {
        // At 60 degrees a pixel covers half the ground it covers at the equator.
        let ratio = metres_per_pixel(60.0, 12.0) / metres_per_pixel(0.0, 12.0);
        assert!((ratio - 0.5).abs() < 1e-9);
    }

    #[test]
    fn bar_picks_a_round_distance_that_fits() {
        // Trier at zoom 12: about 24.6 m per pixel, so 120 px are about 2950 m.
        let bar = scale_bar(49.75, 12.0).unwrap();
        assert_eq!(bar.label, "2 km");
        assert!(bar.length > 60.0 && bar.length <= MAXIMUM_LENGTH);

        // Close in, the bar switches to metres.
        let bar = scale_bar(49.75, 19.0).unwrap();
        assert_eq!(bar.label, "20 m");
        assert!(bar.length <= MAXIMUM_LENGTH);
    }
}
