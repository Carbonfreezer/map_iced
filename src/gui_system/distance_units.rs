//! Distances as text in metric or imperial units, and the conversions between them.
//! Distances are metres everywhere inside the crate; the unit system only decides
//! how they are shown, on the scale bar with [`super::map_widget_system::MapWidgetSystem::set_distance_units`]
//! and in the application's own texts with the functions here.

/// The length of an international foot in metres.
pub const METRES_PER_FOOT: f64 = 0.3048;

/// The length of an international (statute) mile in metres.
pub const METRES_PER_MILE: f64 = 1609.344;

/// Below this many metres [`imperial_distance_text`] counts in feet: a tenth of a
/// mile, 528 feet.
const FEET_LIMIT: f64 = 0.1 * METRES_PER_MILE;

/// The unit system distances are shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DistanceUnits {
    /// Metres, from one kilometre on kilometres.
    #[default]
    Metric,
    /// Feet, from a tenth of a mile on miles.
    Imperial,
}

impl DistanceUnits {
    /// `metres` as text in this unit system, see [`metric_distance_text`] and
    /// [`imperial_distance_text`].
    pub fn distance_text(self, metres: f64) -> String {
        match self {
            DistanceUnits::Metric => metric_distance_text(metres),
            DistanceUnits::Imperial => imperial_distance_text(metres),
        }
    }
}

/// Metres from miles.
pub fn metres_from_miles(miles: f64) -> f64 {
    miles * METRES_PER_MILE
}

/// Metres from feet.
pub fn metres_from_feet(feet: f64) -> f64 {
    feet * METRES_PER_FOOT
}

/// `metres` as text: whole metres below one kilometre, kilometres from there on.
/// E.g. "850 m", "1.24 km", "12.4 km", "124 km".
pub fn metric_distance_text(metres: f64) -> String {
    if metres.abs() < 1000.0 {
        format!("{metres:.0} m")
    } else {
        format!("{} km", three_digits(metres / 1000.0))
    }
}

/// `metres` as text: whole feet below a tenth of a mile, miles from there on.
/// E.g. "420 ft", "0.25 mi", "1.24 mi", "12.4 mi".
pub fn imperial_distance_text(metres: f64) -> String {
    if metres.abs() < FEET_LIMIT {
        format!("{:.0} ft", metres / METRES_PER_FOOT)
    } else {
        format!("{} mi", three_digits(metres / METRES_PER_MILE))
    }
}

/// `value` with two decimals below 10, one below 100 and none above.
fn three_digits(value: f64) -> String {
    let decimals = match value.abs() {
        v if v < 10.0 => 2,
        v if v < 100.0 => 1,
        _ => 0,
    };
    format!("{value:.decimals$}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(metres_from_miles(1.0), 1609.344);
        assert_eq!(metres_from_feet(5280.0), metres_from_miles(1.0));
    }

    #[test]
    fn metric_texts() {
        assert_eq!(metric_distance_text(850.4), "850 m");
        assert_eq!(metric_distance_text(1240.0), "1.24 km");
        assert_eq!(metric_distance_text(12_400.0), "12.4 km");
        assert_eq!(metric_distance_text(124_000.0), "124 km");
    }

    #[test]
    fn imperial_texts() {
        assert_eq!(imperial_distance_text(metres_from_feet(420.0)), "420 ft");
        assert_eq!(imperial_distance_text(metres_from_miles(0.25)), "0.25 mi");
        assert_eq!(imperial_distance_text(metres_from_miles(12.4)), "12.4 mi");
        assert_eq!(
            DistanceUnits::Imperial.distance_text(metres_from_miles(1.24)),
            "1.24 mi"
        );
    }
}
