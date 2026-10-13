//! This system represents regions that can be selected and annotated and zoomed to.

use geo::{point, Area, BoundingRect, Contains, ConvexHull, MultiPoint, Polygon};
use iced::Color;
use slotmap::{new_key_type, SlotMap};
use crate::annotation_system::annotation_support::{Annotation, LabelPosition, MercatorRectangle, RenderingClosedShape, RenderingInformation};
use crate::annotation_system::waypoint_system::{DirectionFlag};
use crate::gui_system::latitude_longitude::LatitudeLongitude;

/// Below this ratio of area to squared size a convex hull counts as a line, see
/// [`RegionShape::ConvexHull`].
const DEGENERATE_AREA_RATIO: f64 = 1e-9;

/// How many points a full circle gets. A sector gets its share of them on the arc.
const RING_POINTS: usize = 128;

new_key_type! {
    /// The handle of a way point inside a [`RegionSystem`]. A type of its own, so
    /// that it cannot be confused with the handles of the other annotation kinds.
    pub struct RegionKey;
}



/// A that may represent different shapes.
#[derive(Debug, Clone)]
pub struct Region {
    /// The mercator rectangle of the region.
    bounds : MercatorRectangle,
    /// The point the region stands for, reported by [`Annotation::get_center`] and
    /// targeted by the direction arrow: the apex of a sector, the centre of a circle,
    /// the centre of the bounds otherwise.
    center: LatitudeLongitude,
    /// The shape the region was built from, everything else here is derived from it.
    shape: RegionShape,
    /// The polygon in geo coordinates.
    geo_polygon : Polygon,
    /// The rendering information.
    rendering_info: RenderingClosedShape,
    /// The annotation text of the region.
    /// An optional string that may be drawn in hover over.
    pub(crate) description: Option<String>,
    /// The direction arrow, if the region is flagged. Set with
    /// [`RegionSystem::set_flag`].
    pub(crate) flag: Option<DirectionFlag>,
}

impl Annotation for Region {
    fn description(&self, _cursor: LatitudeLongitude) -> Option<String> {
        self.description.clone()
    }

    fn cull_bounds(&self) -> &MercatorRectangle {
        &self.bounds
    }

    fn get_center(&self) -> LatitudeLongitude {
        self.center
    }

    fn hit_test_specific(&self, position: LatitudeLongitude) -> bool {
        self.geo_polygon.contains(&point!(x: position.longitude, y: position.latitude))
    }

    fn label_anchor(&self, _cursor: LatitudeLongitude) -> LabelPosition {
        LabelPosition::AtMousePosition
    }

    fn get_flag(&self) -> Option<DirectionFlag> {
        self.flag
    }

    fn get_render_information(&self) -> RenderingInformation<'_> {
        RenderingInformation::Polygon(&self.rendering_info)
    }

    fn clone_box(&self) -> Box<dyn Annotation> {
        Box::new(self.clone())
    }
}

/// The region system for all the different regions.
#[derive(Debug, Clone, Default)]
pub struct RegionSystem {
    pub(crate) region_collection: SlotMap<RegionKey, Region>,
}

/// The errors of the [`RegionSystem`]. A method that fails changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionError {
    /// There is no region with this key, or not any more.
    KeyNotContained,
    /// The shape would span no area, see the variants of [`RegionShape`] for when.
    DegenerateShape,
}

/// Everything a region derives from its [`RegionShape`].
struct Outline {
    /// The open counter-clockwise ring.
    positions_ccw: Vec<LatitudeLongitude>,
    /// The point the region reports as its centre.
    center: LatitudeLongitude,
    /// The bounds of the ring.
    bounds: MercatorRectangle,
    /// The ring as polygon for the hit test.
    geo_polygon: Polygon,
}

/// The colours a region is drawn with.
#[derive(Debug, Clone, Copy)]
pub struct RegionStyle {
    /// The colour the inside is filled with, usually translucent.
    pub fill: Color,
    /// The colour of the outline.
    pub border: Color,
}

/// The shape of a region, as it was handed to [`RegionSystem::add_region`]. The region
/// keeps it, so it can be asked for later with [`RegionSystem::shape`].
///
/// All rings built from it run counter-clockwise with longitude as x and latitude as
/// y, see [`RenderingClosedShape`].
#[derive(Debug, Clone)]
pub enum RegionShape {
    /// A circle of `radius_meter` around `center`, measured along great circles. The
    /// region reports `center` as its centre.
    Circle {
        center: LatitudeLongitude,
        radius_meter: f64,
    },
    /// The convex hull of `points`, computed with longitude as x and latitude as y, the
    /// same plane the hit test works in. Points inside the hull are dropped, so their
    /// order does not matter. Fewer than three points, or points on a single line, span
    /// no area. The region reports the centre of its bounds as its centre.
    ConvexHull { points: Vec<LatitudeLongitude> },
    /// The fan of `radius_meter` around `apex`, opened by `opening_degree` and centred
    /// on `heading_degree`, the compass bearing it points to, clockwise from north.
    ///
    /// The heading is where the sector points *to*. A wind direction is reported as
    /// where the wind comes *from*, gas carried by a 270 degree wind spreads towards
    /// 90 degrees. The opening has to lie in `0 < opening <= 180`, which keeps the
    /// sector convex.
    ///
    /// The region reports the apex as its centre, so a direction arrow points at the
    /// lighthouse or the leak rather than into the fan.
    Sector {
        apex: LatitudeLongitude,
        radius_meter: f64,
        heading_degree: f64,
        opening_degree: f64,
    },
}

impl RegionShape {
    /// Everything the region derives from the shape, see [`Self::ring`] for when it
    /// fails.
    fn outline(&self) -> Result<Outline, RegionError> {
        let (positions_ccw, center) = self.ring()?;
        let geo_polygon = Polygon::new(positions_ccw.iter().map(|pos|(pos.longitude, pos.latitude)).collect() ,vec![]);
        let bounds = MercatorRectangle::create_from_position_array(positions_ccw.iter().copied(), 0.0);
        Ok(Outline {
            center: center.unwrap_or_else(|| bounds.get_center()),
            positions_ccw,
            bounds,
            geo_polygon,
        })
    }

    /// The open counter-clockwise ring of the shape and the point the region reports
    /// as its centre, `None` for the centre of the bounds. A shape that spans no area
    /// gives [`RegionError::DegenerateShape`]; a radius that is not positive counts as such.
    fn ring(&self) -> Result<(Vec<LatitudeLongitude>, Option<LatitudeLongitude>), RegionError> {
        match self {
            RegionShape::Circle { center, radius_meter } => {
                if radius_meter.is_nan() || *radius_meter <= 0.0 {
                    return Err(RegionError::DegenerateShape);
                }
                Ok((center.create_circle_around(*radius_meter, RING_POINTS), Some(*center)))
            }
            RegionShape::ConvexHull { points } => {
                let points: MultiPoint = points
                    .iter()
                    .map(|pos| (pos.longitude, pos.latitude))
                    .collect::<Vec<_>>()
                    .into();
                let hull = points.convex_hull();
                // Points on one line still leave an area from rounding, so the area is
                // taken relative to the squared size of the hull. Even a thin corridor,
                // ten metres wide over ten kilometres, stays orders of magnitude above
                // the threshold.
                let size_squared = hull
                    .bounding_rect()
                    .map_or(0.0, |rect| rect.width().powi(2) + rect.height().powi(2));
                if hull.unsigned_area() <= size_squared * DEGENERATE_AREA_RATIO {
                    return Err(RegionError::DegenerateShape);
                }
                // geo hands the exterior over counter-clockwise, but closed: the first
                // point is repeated at the end, our rings are open.
                let ring = hull.exterior().0.as_slice();
                let positions = ring[..ring.len() - 1]
                    .iter()
                    .map(|coord| LatitudeLongitude::new(coord.y, coord.x))
                    .collect();
                Ok((positions, None))
            }
            RegionShape::Sector { apex, radius_meter, heading_degree, opening_degree } => {
                if !(*opening_degree > 0.0 && *opening_degree <= 180.0 && *radius_meter > 0.0) {
                    return Err(RegionError::DegenerateShape);
                }
                let arc_points = (RING_POINTS as f64 * opening_degree / 360.0).ceil() as usize + 1;
                let positions =
                    apex.create_sector_around(*radius_meter, *heading_degree, *opening_degree, arc_points);
                Ok((positions, Some(*apex)))
            }
        }
    }
}

impl RegionSystem {
    // TODO: Examples need to get added.-

    /// Adds a region of the given shape and returns the key. A shape that spans no area
    /// gives [`RegionError::DegenerateShape`], see the variants of [`RegionShape`].
    pub fn add_region(
        &mut self,
        shape: RegionShape,
        style: RegionStyle,
        description: Option<String>,
    ) -> Result<RegionKey, RegionError> {
        let outline = shape.outline()?;
        let region = Region {
            center: outline.center,
            bounds: outline.bounds,
            shape,
            geo_polygon: outline.geo_polygon,
            rendering_info: RenderingClosedShape {
                positions_ccw: outline.positions_ccw,
                core_color: style.fill,
                border_color: style.border,
            },
            description,
            flag: None
        };
        Ok(self.region_collection.insert(region))
    }

    /// The current shape of the region, as handed to [`Self::add_region`] or
    /// [`Self::update_shape`].
    pub fn shape(&self, key: RegionKey) -> Result<&RegionShape, RegionError> {
        Ok(&self.region_collection.get(key).ok_or(RegionError::KeyNotContained)?.shape)
    }

    /// Gives the region a new shape, of any kind. Key, style, description and flag stay,
    /// so the region keeps being the same thing for the application and keeps a
    /// selection in the widgets, like a gas plume whose forecast is renewed every hour.
    /// Something that has become a different thing is better deleted and added anew.
    ///
    /// A shape that spans no area leaves the region unchanged.
    pub fn update_shape(&mut self, key: RegionKey, shape: RegionShape) -> Result<(), RegionError> {
        let region = self
            .region_collection
            .get_mut(key)
            .ok_or(RegionError::KeyNotContained)?;
        let outline = shape.outline()?;
        region.center = outline.center;
        region.bounds = outline.bounds;
        region.geo_polygon = outline.geo_polygon;
        region.rendering_info.positions_ccw = outline.positions_ccw;
        region.shape = shape;
        Ok(())
    }

    /// Changes the colours of the region.
    pub fn update_style(&mut self, key: RegionKey, style: RegionStyle) -> Result<(), RegionError> {
        let render_info = &mut self.region_collection.get_mut(key).ok_or(RegionError::KeyNotContained)?.rendering_info;
        render_info.core_color = style.fill;
        render_info.border_color = style.border;
        Ok(())
    }

    /// Sets a new annotation description.
    pub fn update_description(&mut self, key: RegionKey, description : Option<String>) -> Result<(), RegionError> {
        self.region_collection
            .get_mut(key)
            .ok_or(RegionError::KeyNotContained)?
            .description = description;
        Ok(())
    }

    /// Sets the arrow flag for the region.
    pub fn set_flag(&mut self, key: RegionKey, flag: Option<DirectionFlag>) -> Result<(), RegionError> {
        self.region_collection.get_mut(key).ok_or(RegionError::KeyNotContained)?.flag = flag;
        Ok(())
    }

    /// Removes the region key from the list.
    pub fn delete_region(&mut self, key: RegionKey) -> Result<(), RegionError> {
        let deletion = self.region_collection.remove(key);
        if deletion.is_some() {
            Ok(())
        } else {
            Err(RegionError::KeyNotContained)
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLE: RegionStyle = RegionStyle {
        fill: Color::WHITE,
        border: Color::BLACK,
    };

    fn add(system: &mut RegionSystem, points: &[(f64, f64)]) -> Result<RegionKey, RegionError> {
        let points = points.iter().map(|&(lat, long)| LatitudeLongitude::new(lat, long)).collect();
        system.add_region(RegionShape::ConvexHull { points }, STYLE, None)
    }

    #[test]
    fn hull_drops_inner_points_and_stays_open_and_ccw() {
        let mut system = RegionSystem::default();
        // A square around Trier with one point inside, in no particular order.
        let key = add(
            &mut system,
            &[(49.8, 6.7), (49.7, 6.6), (49.75, 6.65), (49.7, 6.7), (49.8, 6.6)],
        )
        .expect("the square spans an area");
        let region = &system.region_collection[key];
        let ring = &region.rendering_info.positions_ccw;
        assert_eq!(ring.len(), 4, "inner point dropped, no repeated closing point");

        // Twice the signed area, longitude as x and latitude as y.
        let doubled_area: f64 = ring
            .iter()
            .zip(ring.iter().cycle().skip(1))
            .map(|(a, b)| a.longitude * b.latitude - b.longitude * a.latitude)
            .sum();
        assert!(doubled_area > 0.0);

        assert_eq!(region.bounds.min_max_lat, (49.7, 49.8));
        assert_eq!(region.bounds.min_max_long, (6.6, 6.7));
        assert!(region.hit_test_specific(LatitudeLongitude::new(49.75, 6.65)));
        assert!(!region.hit_test_specific(LatitudeLongitude::new(49.75, 6.75)));
    }

    #[test]
    fn degenerate_points_give_no_region() {
        let mut system = RegionSystem::default();
        assert!(add(&mut system, &[]).is_err());
        assert!(add(&mut system, &[(49.7, 6.6), (49.8, 6.7)]).is_err());
        assert!(add(&mut system, &[(49.7, 6.6), (49.75, 6.65), (49.8, 6.7)]).is_err());
        assert!(add(&mut system, &[(49.7, 6.6), (49.7, 6.6), (49.7, 6.6)]).is_err());
        assert!(system.region_collection.is_empty());
    }

    fn add_sector(system: &mut RegionSystem, radius: f64, opening: f64) -> Result<RegionKey, RegionError> {
        let shape = RegionShape::Sector {
            apex: LatitudeLongitude::new(49.75, 6.63),
            radius_meter: radius,
            heading_degree: 90.0,
            opening_degree: opening,
        };
        system.add_region(shape, STYLE, None)
    }

    #[test]
    fn sector_points_east_and_reports_its_apex() {
        let mut system = RegionSystem::default();
        let key = add_sector(&mut system, 10_000.0, 60.0).expect("a valid sector");
        let region = &system.region_collection[key];
        assert_eq!(region.get_center().latitude, 49.75);
        assert_eq!(region.get_center().longitude, 6.63);
        // 0.07 degrees of longitude are about 5 km at this latitude.
        assert!(region.hit_test_specific(LatitudeLongitude::new(49.75, 6.70)));
        assert!(!region.hit_test_specific(LatitudeLongitude::new(49.75, 6.56)));
        assert!(!region.hit_test_specific(LatitudeLongitude::new(49.80, 6.63)));
        // The bounds start at the apex and reach east only.
        assert_eq!(region.bounds.min_max_long.0, 6.63);
    }

    #[test]
    fn sector_limits() {
        let mut system = RegionSystem::default();
        assert!(add_sector(&mut system, 10_000.0, 180.0).is_ok());
        assert!(add_sector(&mut system, 10_000.0, 0.0).is_err());
        assert!(add_sector(&mut system, 10_000.0, 180.5).is_err());
        assert!(add_sector(&mut system, 10_000.0, f64::NAN).is_err());
        assert!(add_sector(&mut system, 0.0, 60.0).is_err());
        assert_eq!(system.region_collection.len(), 1);
    }

    #[test]
    fn circle_reports_its_own_centre_and_shape() {
        let mut system = RegionSystem::default();
        let center = LatitudeLongitude::new(49.75, 6.63);
        let shape = RegionShape::Circle { center, radius_meter: 10_000.0 };
        let key = system.add_region(shape, STYLE, None).expect("a valid circle");
        let reported = system.region_collection[key].get_center();
        assert_eq!((reported.latitude, reported.longitude), (49.75, 6.63));
        assert!(matches!(
            system.shape(key),
            Ok(RegionShape::Circle { radius_meter, .. }) if *radius_meter == 10_000.0
        ));

        system.delete_region(key).expect("present");
        assert!(system.shape(key).is_err());
    }

    #[test]
    fn update_shape_keeps_the_region_and_rebuilds_its_outline() {
        let mut system = RegionSystem::default();
        let apex = LatitudeLongitude::new(49.75, 6.63);
        let sector = |heading_degree| RegionShape::Sector {
            apex,
            radius_meter: 10_000.0,
            heading_degree,
            opening_degree: 60.0,
        };
        let key = system
            .add_region(sector(90.0), STYLE, Some("plume".to_string()))
            .expect("a valid sector");
        let east = LatitudeLongitude::new(49.75, 6.70);
        let west = LatitudeLongitude::new(49.75, 6.56);
        assert!(system.region_collection[key].hit_test_specific(east));

        // The wind turns, the plume now spreads west.
        system.update_shape(key, sector(270.0)).expect("a valid sector");
        let region = &system.region_collection[key];
        assert!(region.hit_test_specific(west));
        assert!(!region.hit_test_specific(east));
        assert_eq!(region.bounds.min_max_long.1, 6.63);
        assert_eq!(region.description.as_deref(), Some("plume"));
        assert!(matches!(
            system.shape(key),
            Ok(RegionShape::Sector { heading_degree, .. }) if *heading_degree == 270.0
        ));

        // Another kind of shape is fine too, and reports its own centre.
        let points = vec![
            LatitudeLongitude::new(49.7, 6.6),
            LatitudeLongitude::new(49.7, 6.8),
            LatitudeLongitude::new(49.9, 6.6),
        ];
        system
            .update_shape(key, RegionShape::ConvexHull { points })
            .expect("a valid triangle");
        let center = system.region_collection[key].get_center();
        assert!((center.latitude - 49.8).abs() < 1e-9 && (center.longitude - 6.7).abs() < 1e-9);
    }

    #[test]
    fn failed_update_shape_leaves_the_region_alone() {
        let mut system = RegionSystem::default();
        let center = LatitudeLongitude::new(49.75, 6.63);
        let key = system
            .add_region(RegionShape::Circle { center, radius_meter: 10_000.0 }, STYLE, None)
            .expect("a valid circle");
        let ring_before = system.region_collection[key].rendering_info.positions_ccw.len();

        let flat = RegionShape::Circle { center, radius_meter: 0.0 };
        assert_eq!(system.update_shape(key, flat), Err(RegionError::DegenerateShape));
        assert_eq!(system.region_collection[key].rendering_info.positions_ccw.len(), ring_before);
        assert!(matches!(system.shape(key), Ok(RegionShape::Circle { radius_meter, .. }) if *radius_meter == 10_000.0));

        system.delete_region(key).expect("present");
        let valid = RegionShape::Circle { center, radius_meter: 10_000.0 };
        assert_eq!(system.update_shape(key, valid), Err(RegionError::KeyNotContained));
    }

    #[test]
    fn circle_needs_a_positive_radius() {
        let mut system = RegionSystem::default();
        let center = LatitudeLongitude::new(49.75, 6.63);
        for radius_meter in [0.0, -5.0, f64::NAN] {
            let shape = RegionShape::Circle { center, radius_meter };
            assert!(system.add_region(shape, STYLE, None).is_err());
        }
        assert!(system.region_collection.is_empty());
    }
}
