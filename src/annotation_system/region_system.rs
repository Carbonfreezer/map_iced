//! This system represents regions that can be selected and annotated and zoomed to.

use geo::{point, Area, BoundingRect, Contains, ConvexHull, MultiPoint, Polygon};
use iced::Color;
use slotmap::{new_key_type, SlotMap};
use crate::annotation_system::annotation_support::{Annotation, LabelPosition, MercatorRectangle, RenderingClosedShape, RenderingInformation};
use crate::annotation_system::waypoint_system::{DirectionFlag};
use crate::gui_system::latitude_longitude::LatitudeLongitude;

/// Below this ratio of area to squared size a convex hull counts as a line, see
/// [`RegionSystem::add_convex_region`].
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

/// Error type for that case that the region is not contained in the structure.
#[derive(Debug, Clone, Copy)]
pub struct RegionKeyNotContained;

/// Error type for a region that would span no area, see the method that returned it.
#[derive(Debug, Clone, Copy)]
pub struct DegenerateRegion;

impl RegionSystem {
    // TODO: Examples need to get added.-

    /// Adds a circular region to the system and returns the key.
    pub fn add_circular_region(
        &mut self,
        position: LatitudeLongitude,
        radius: f64,
        inner_color : Color,
        border_color : Color,
        description: Option<String>,
    ) -> RegionKey {
        self.insert_region(position.create_circle_around(radius, RING_POINTS), Some(position), inner_color, border_color, description)
    }

    /// Adds the convex hull of `positions` as a region and returns the key.
    ///
    /// The hull is computed with longitude as x and latitude as y, the same plane the
    /// hit test works in. Points inside the hull are dropped, so the order they come
    /// in does not matter. Fewer than three points, or points on a single line, span
    /// no area and give [`DegenerateRegion`].
    pub fn add_convex_region(
        &mut self,
        positions: impl Iterator<Item = LatitudeLongitude>,
        inner_color : Color,
        border_color : Color,
        description: Option<String>,
    ) -> Result<RegionKey, DegenerateRegion> {
        let points: MultiPoint = positions
            .map(|pos| (pos.longitude, pos.latitude))
            .collect::<Vec<_>>()
            .into();
        let hull = points.convex_hull();
        // Points on one line still leave an area from rounding, so the area is taken
        // relative to the squared size of the hull. Even a thin corridor, ten metres
        // wide over ten kilometres, stays orders of magnitude above the threshold.
        let size_squared = hull
            .bounding_rect()
            .map_or(0.0, |rect| rect.width().powi(2) + rect.height().powi(2));
        if hull.unsigned_area() <= size_squared * DEGENERATE_AREA_RATIO {
            return Err(DegenerateRegion);
        }
        // geo hands the exterior over counter-clockwise, but closed: the first point
        // is repeated at the end, our rings are open.
        let ring = hull.exterior().0.as_slice();
        let positions_ccw = ring[..ring.len() - 1]
            .iter()
            .map(|coord| LatitudeLongitude::new(coord.y, coord.x))
            .collect();
        Ok(self.insert_region(positions_ccw, None, inner_color, border_color, description))
    }

    /// Adds a circle sector as a region and returns the key: the fan of `radius` meters
    /// around `apex`, opened by `opening_degree` and centred on `heading_degree`, the
    /// compass bearing it points to, clockwise from north.
    ///
    /// The heading is where the sector points *to*. A wind direction is reported as
    /// where the wind comes *from*, gas carried by a 270 degree wind spreads towards
    /// 90 degrees.
    ///
    /// The apex is what the region reports as its centre, so a direction arrow points
    /// at the lighthouse or the leak rather than into the fan. An opening outside
    /// `0 < opening <= 180`, or a radius that is not positive, gives
    /// [`DegenerateRegion`]; the limit keeps the sector convex.
    pub fn add_sector_region(
        &mut self,
        apex: LatitudeLongitude,
        radius: f64,
        heading_degree: f64,
        opening_degree: f64,
        inner_color : Color,
        border_color : Color,
        description: Option<String>,
    ) -> Result<RegionKey, DegenerateRegion> {
        if !(opening_degree > 0.0 && opening_degree <= 180.0 && radius > 0.0) {
            return Err(DegenerateRegion);
        }
        let arc_points = (RING_POINTS as f64 * opening_degree / 360.0).ceil() as usize + 1;
        let positions_ccw =
            apex.create_sector_around(radius, heading_degree, opening_degree, arc_points);
        Ok(self.insert_region(positions_ccw, Some(apex), inner_color, border_color, description))
    }

    /// Builds the region from an open counter-clockwise ring and stores it. `center` is
    /// what the region reports as its centre, the centre of its bounds if `None`.
    fn insert_region(
        &mut self,
        positions_ccw: Vec<LatitudeLongitude>,
        center: Option<LatitudeLongitude>,
        inner_color : Color,
        border_color : Color,
        description: Option<String>,
    ) -> RegionKey {
        let rendering_info = RenderingClosedShape {
            positions_ccw,
            core_color: inner_color,
            border_color,
        };

        let geo_polygon = Polygon::new(rendering_info.positions_ccw.iter().map(|pos|(pos.longitude, pos.latitude)).collect() ,vec![]);
        let bounds = MercatorRectangle::create_from_position_array(rendering_info.positions_ccw.iter().copied(), 0.0);

        let region = Region {
            center: center.unwrap_or_else(|| bounds.get_center()),
            bounds,
            geo_polygon,
            rendering_info,
            description,
            flag: None
        };
        self.region_collection.insert(region)
    }

    /// Changes the coloring of the region.
    pub fn update_color(&mut self, key: RegionKey, inner_color : Color, border_color : Color) -> Result<(), RegionKeyNotContained> {
        let render_info = &mut self.region_collection.get_mut(key).ok_or(RegionKeyNotContained)?.rendering_info;
        render_info.core_color = inner_color;
        render_info.border_color = border_color;
        Ok(())
    }

    /// Sets a new annotation description.
    pub fn update_description(&mut self, key: RegionKey, description : Option<String>) -> Result<(), RegionKeyNotContained> {
        self.region_collection
            .get_mut(key)
            .ok_or(RegionKeyNotContained)?
            .description = description;
        Ok(())
    }

    /// Sets the arrow flag for the region.
    pub fn set_flag(&mut self, key: RegionKey, flag: Option<DirectionFlag>) -> Result<(), RegionKeyNotContained> {
        self.region_collection.get_mut(key).ok_or(RegionKeyNotContained)?.flag = flag;
        Ok(())
    }

    /// Removes the region key from the list.
    pub fn delete_region(&mut self, key: RegionKey) -> Result<(), RegionKeyNotContained> {
        let deletion = self.region_collection.remove(key);
        if deletion.is_some() {
            Ok(())
        } else {
            Err(RegionKeyNotContained)
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(system: &mut RegionSystem, points: &[(f64, f64)]) -> Result<RegionKey, DegenerateRegion> {
        system.add_convex_region(
            points.iter().map(|&(lat, long)| LatitudeLongitude::new(lat, long)),
            Color::WHITE,
            Color::BLACK,
            None,
        )
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

    fn add_sector(system: &mut RegionSystem, radius: f64, opening: f64) -> Result<RegionKey, DegenerateRegion> {
        system.add_sector_region(
            LatitudeLongitude::new(49.75, 6.63),
            radius,
            90.0,
            opening,
            Color::WHITE,
            Color::BLACK,
            None,
        )
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
    fn circle_reports_its_own_centre() {
        let mut system = RegionSystem::default();
        let centre = LatitudeLongitude::new(49.75, 6.63);
        let key = system.add_circular_region(centre, 10_000.0, Color::WHITE, Color::BLACK, None);
        let reported = system.region_collection[key].get_center();
        assert_eq!((reported.latitude, reported.longitude), (49.75, 6.63));
    }
}
