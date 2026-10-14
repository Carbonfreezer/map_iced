//! This module administrates paths, which typically stem from gpx tracks and are a series of
//! points that are annotated with a time stamp.

use crate::annotation_system::annotation_support::{Annotation, ClosestPoint, DirectionFlag, LabelPosition, MercatorRectangle, RenderingInformation};
use crate::gui_system::internal_math::MercatorPoint;
use crate::gui_system::latitude_longitude::LatitudeLongitude;
use geo::{Densify, Haversine, LineString};
use iced::Color;
use slotmap::{SlotMap, new_key_type};
use std::sync::Arc;

/// The longest segment a path gets drawn and hit tested with, in metres. Longer ones
/// get points inserted along the great circle.
///
/// A segment is drawn as a straight line in Mercator space, a rhumb line, which
/// deviates from the great circle by about `L² · tan(latitude) / (8 · R)` at most:
/// for 10 km that is about 2 m at 50 degrees and 11 m at 80 degrees, below a pixel
/// unless the view is zoomed close onto such a segment. A densely sampled GPS track
/// never reaches the limit, only sparse ones like routes or flights do.
const MAX_SEGMENT_METER: f64 = 10_000.0;

/// How close in pixels the cursor has to come to the line of a path to hit it. It
/// is the pixel padding of the path's [`MercatorRectangle`], so it also keeps the
/// line from being culled while its stroke still reaches into the view.
const PATH_HOVER_PADDING: f64 = 8.0;

new_key_type! {
    /// The handle of a path inside a [`PathSystem`]. A type of its own, so that it
    /// cannot be confused with the handles of the other annotation kinds.
    ///
    /// It is `Copy`, `Eq`, `Hash` and `Ord`, so data of your own about a path can
    /// live in a `HashMap` or `BTreeMap` keyed by it.
    pub struct PathKey;
}

/// The errors of the [`PathSystem`]. A method that fails changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// There is no path with this key, or not any more.
    KeyNotContained,
    /// A path needs at least two positions, a single one is a way point.
    TooFewPositions,
}

/// A point on a path, as found by [`PathSystem::closest_point_on_path`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathLocation {
    /// The point on the path.
    pub position: LatitudeLongitude,
    /// How far it lies from the start of the path, measured along the path with
    /// the haversine formula, in metres.
    pub distance_from_start_meter: f64,
}

/// Everything a path derives from its positions. Shared behind an [`Arc`], because
/// the widgets take a snapshot of every annotation in view with each change of
/// their view, and a track can have tens of thousands of positions.
#[derive(Debug)]
struct PathGeometry {
    /// The positions as handed over.
    positions: Vec<LatitudeLongitude>,
    /// The positions with the points inserted on long segments, these get drawn.
    dense_positions: Vec<LatitudeLongitude>,
    /// `dense_positions` in Mercator space, for the hit test.
    mercator: Vec<MercatorPoint>,
    /// For every dense position the haversine length of the path up to it, in metres.
    distance_to_vertex: Vec<f64>,
    /// The bounds of the dense positions. Those decide, a great circle can bulge
    /// beyond the positions it connects.
    bounds: MercatorRectangle,
    /// The point on the path closest to the centre of the bounds.
    center: LatitudeLongitude,
}

/// Where a position projects onto a path: the segment from dense position
/// `segment` to the next one, and the point on it.
struct Projection {
    segment: usize,
    point: MercatorPoint,
}

impl PathGeometry {
    /// Builds the geometry, see [`PathError::TooFewPositions`] for when it fails.
    fn new(positions: Vec<LatitudeLongitude>) -> Result<Self, PathError> {
        if positions.len() < 2 {
            return Err(PathError::TooFewPositions);
        }
        let line: LineString = positions
            .iter()
            .map(|pos| (pos.longitude, pos.latitude))
            .collect::<Vec<_>>()
            .into();
        let dense_positions: Vec<_> = Haversine
            .densify(&line, MAX_SEGMENT_METER)
            .coords()
            .map(|coord| LatitudeLongitude::new(coord.y, coord.x))
            .collect();
        let mercator = dense_positions.iter().map(|&pos| pos.into()).collect();
        let distance_to_vertex = std::iter::once(0.0)
            .chain(dense_positions.windows(2).scan(0.0, |sum, pair| {
                *sum += pair[0].get_distance_to(pair[1]);
                Some(*sum)
            }))
            .collect();
        let bounds = MercatorRectangle::create_from_position_array(
            dense_positions.iter().copied(),
            PATH_HOVER_PADDING,
        )
        .expect("a path has at least two positions");
        let mut geometry = Self {
            positions,
            dense_positions,
            mercator,
            distance_to_vertex,
            center: bounds.get_center(),
            bounds,
        };
        geometry.center = geometry.project(geometry.center.into()).point.into();
        Ok(geometry)
    }

    /// The point on the path closest to `position`, Euclidean in Mercator space, the
    /// way the path is seen on screen.
    fn project(&self, position: MercatorPoint) -> Projection {
        self.mercator
            .windows(2)
            .enumerate()
            .map(|(segment, pair)| {
                let (start, end) = (pair[0], pair[1]);
                let (dx, dy) = (end.x - start.x, end.y - start.y);
                let length_squared = dx * dx + dy * dy;
                // Repeated positions, common in GPS tracks, give segments of no length.
                let fraction = if length_squared > 0.0 {
                    (((position.x - start.x) * dx + (position.y - start.y) * dy) / length_squared)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let point = MercatorPoint {
                    x: start.x + fraction * dx,
                    y: start.y + fraction * dy,
                };
                let distance = (point.x - position.x).hypot(point.y - position.y);
                (distance, Projection { segment, point })
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, projection)| projection)
            .expect("a path has at least two positions")
    }

    /// The point on the path closest to `position` and how far along the path it lies.
    fn locate(&self, position: LatitudeLongitude) -> PathLocation {
        let Projection { segment, point } = self.project(position.into());
        let position = LatitudeLongitude::from(point);
        PathLocation {
            position,
            distance_from_start_meter: self.distance_to_vertex[segment]
                + self.dense_positions[segment].get_distance_to(position),
        }
    }
}

/// A path drawn as a line in one colour.
#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    /// The geometry, shared with the snapshots the widgets take.
    geometry: Arc<PathGeometry>,
    /// The colour of the line.
    color: Color,
    /// The annotation text of the path.
    /// An optional string that may be drawn in hover over.
    pub(crate) description: Option<String>,
    /// The direction arrow, if the path is flagged. Set with [`PathSystem::set_flag`].
    pub(crate) flag: Option<DirectionFlag>,
}

impl Annotation for PathInfo {
    fn description(&self, _cursor: LatitudeLongitude) -> Option<String> {
        self.description.clone()
    }

    fn cull_bounds(&self) -> &MercatorRectangle {
        &self.geometry.bounds
    }

    fn get_center(&self) -> LatitudeLongitude {
        self.geometry.center
    }

    fn closest_point(&self, position: MercatorPoint) -> ClosestPoint {
        ClosestPoint::Nearest(self.geometry.project(position).point)
    }

    /// The label sits on the line, next to the cursor.
    fn label_anchor(&self, cursor: LatitudeLongitude) -> LabelPosition {
        LabelPosition::FixGeoLocation(self.geometry.project(cursor.into()).point.into())
    }

    fn get_flag(&self) -> Option<DirectionFlag> {
        self.flag
    }

    fn get_render_information(&self) -> RenderingInformation<'_> {
        RenderingInformation::Line {
            positions: &self.geometry.dense_positions,
            color: self.color,
        }
    }

    fn clone_box(&self) -> Box<dyn Annotation> {
        Box::new(self.clone())
    }
}

/// The path system for all the different paths.
#[derive(Debug, Clone, Default)]
pub struct PathSystem {
    pub(crate) path_collection: SlotMap<PathKey, PathInfo>,
}

impl PathSystem {
    /// Adds a path through the positions, in this order, drawn as a line in `color`.
    /// Segments longer than 10 km get points inserted along the great circle for
    /// drawing, [`Self::positions`] still gives the positions as handed over.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::{PathError, PathSystem};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// let track = vec![
    ///     LatitudeLongitude::new(49.750, 6.630),
    ///     LatitudeLongitude::new(49.755, 6.640),
    ///     LatitudeLongitude::new(49.760, 6.645),
    /// ];
    /// let key = system.add_path(track, Color::from_rgb(0.0, 0.0, 1.0), Some("Patrol".to_string()));
    /// assert!(key.is_ok());
    ///
    /// // A single position is a way point, not a path.
    /// let single = vec![LatitudeLongitude::new(49.75, 6.63)];
    /// assert_eq!(system.add_path(single, Color::BLACK, None).err(), Some(PathError::TooFewPositions));
    /// ```
    pub fn add_path(
        &mut self,
        positions: Vec<LatitudeLongitude>,
        color: Color,
        description: Option<String>,
    ) -> Result<PathKey, PathError> {
        let path = PathInfo {
            geometry: Arc::new(PathGeometry::new(positions)?),
            color,
            description,
            flag: None,
        };
        Ok(self.path_collection.insert(path))
    }

    /// The positions of the path, as handed to [`Self::add_path`] or
    /// [`Self::update_positions`].
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::PathSystem;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// let track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.add_path(track.clone(), Color::BLACK, None).expect("two positions");
    /// assert_eq!(system.positions(key), Ok(track.as_slice()));
    /// ```
    pub fn positions(&self, key: PathKey) -> Result<&[LatitudeLongitude], PathError> {
        Ok(&self
            .path_collection
            .get(key)
            .ok_or(PathError::KeyNotContained)?
            .geometry
            .positions)
    }

    /// The point on the path closest to `position`, and how far along the path it lies
    /// from the start, for instance to report where on a track a click landed.
    ///
    /// Closest means as seen on the map: the distance gets measured in the Mercator
    /// projection, the same way the widgets decide whether the cursor is on the path,
    /// so the point is the one the hover label sits at. The distance along the path
    /// is measured with the haversine formula.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::PathSystem;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// // About 7.2 km due east along the 50th parallel.
    /// let track = vec![LatitudeLongitude::new(50.0, 7.0), LatitudeLongitude::new(50.0, 7.1)];
    /// let key = system.add_path(track, Color::BLACK, None).expect("two positions");
    /// // A click a bit north of the middle of the track.
    /// let location = system
    ///     .closest_point_on_path(key, LatitudeLongitude::new(50.01, 7.05))
    ///     .expect("present");
    /// assert!((location.position.longitude - 7.05).abs() < 1e-9);
    /// assert!((location.distance_from_start_meter - 3_577.0).abs() < 10.0);
    /// ```
    pub fn closest_point_on_path(
        &self,
        key: PathKey,
        position: LatitudeLongitude,
    ) -> Result<PathLocation, PathError> {
        Ok(self
            .path_collection
            .get(key)
            .ok_or(PathError::KeyNotContained)?
            .geometry
            .locate(position))
    }

    /// The point the path stands for: the point on the path closest to the centre of
    /// its bounds. Direction arrows point at it.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::PathSystem;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// // An L shape, east then north. The centre of its bounds lies off the path, and
    /// // on the map the northern leg is nearer to it than the eastern one.
    /// let track = vec![
    ///     LatitudeLongitude::new(50.0, 7.0),
    ///     LatitudeLongitude::new(50.0, 7.1),
    ///     LatitudeLongitude::new(50.1, 7.1),
    /// ];
    /// let key = system.add_path(track, Color::BLACK, None).expect("three positions");
    /// let center = system.center(key).expect("present");
    /// assert!((center.longitude - 7.1).abs() < 1e-9);
    /// assert!((center.latitude - 50.05).abs() < 1e-9);
    /// ```
    pub fn center(&self, key: PathKey) -> Result<LatitudeLongitude, PathError> {
        Ok(self
            .path_collection
            .get(key)
            .ok_or(PathError::KeyNotContained)?
            .geometry
            .center)
    }

    /// Gives the path new positions, like a track that has grown. Key, colour,
    /// description and flag stay. Fewer than two positions leave the path unchanged.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::PathSystem;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// let mut track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.add_path(track.clone(), Color::BLACK, None).expect("two positions");
    /// // The vehicle has moved on.
    /// track.push(LatitudeLongitude::new(49.77, 6.64));
    /// assert!(system.update_positions(key, track).is_ok());
    /// ```
    pub fn update_positions(
        &mut self,
        key: PathKey,
        positions: Vec<LatitudeLongitude>,
    ) -> Result<(), PathError> {
        let path = self
            .path_collection
            .get_mut(key)
            .ok_or(PathError::KeyNotContained)?;
        path.geometry = Arc::new(PathGeometry::new(positions)?);
        Ok(())
    }

    /// Changes the colour of the line.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::PathSystem;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// let track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.add_path(track, Color::BLACK, None).expect("two positions");
    /// assert!(system.update_color(key, Color::from_rgb(1.0, 0.5, 0.0)).is_ok());
    /// ```
    pub fn update_color(&mut self, key: PathKey, color: Color) -> Result<(), PathError> {
        self.path_collection
            .get_mut(key)
            .ok_or(PathError::KeyNotContained)?
            .color = color;
        Ok(())
    }

    /// Sets a new annotation description.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::PathSystem;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// let track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.add_path(track, Color::BLACK, None).expect("two positions");
    /// assert!(system.update_description(key, Some("Patrol".to_string())).is_ok());
    /// ```
    pub fn update_description(
        &mut self,
        key: PathKey,
        description: Option<String>,
    ) -> Result<(), PathError> {
        self.path_collection
            .get_mut(key)
            .ok_or(PathError::KeyNotContained)?
            .description = description;
        Ok(())
    }

    /// Flags the path, or takes the flag away with `None`. A flagged path gets an
    /// arrow on the edge of every widget it is off screen in, pointing at its centre.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::PathSystem;
    /// use map_iced::annotation_system::annotation_support::DirectionFlag;
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// let track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.add_path(track, Color::BLACK, None).expect("two positions");
    /// let flag = DirectionFlag { color: Color::from_rgb(1.0, 0.0, 0.0), priority: 3 };
    /// assert!(system.set_flag(key, Some(flag)).is_ok());
    /// ```
    pub fn set_flag(&mut self, key: PathKey, flag: Option<DirectionFlag>) -> Result<(), PathError> {
        self.path_collection
            .get_mut(key)
            .ok_or(PathError::KeyNotContained)?
            .flag = flag;
        Ok(())
    }

    /// Removes the path with the indicated key.
    ///
    /// # Example
    /// ```
    /// use iced::Color;
    /// use map_iced::annotation_system::path_system::{PathError, PathSystem};
    /// use map_iced::gui_system::LatitudeLongitude;
    /// let mut system = PathSystem::default();
    /// let track = vec![LatitudeLongitude::new(49.75, 6.63), LatitudeLongitude::new(49.76, 6.64)];
    /// let key = system.add_path(track, Color::BLACK, None).expect("two positions");
    /// assert!(system.delete_path(key).is_ok());
    /// assert_eq!(system.delete_path(key), Err(PathError::KeyNotContained));
    /// ```
    pub fn delete_path(&mut self, key: PathKey) -> Result<(), PathError> {
        self.path_collection
            .remove(key)
            .map(|_| ())
            .ok_or(PathError::KeyNotContained)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(points: &[(f64, f64)]) -> PathGeometry {
        PathGeometry::new(
            points
                .iter()
                .map(|&(lat, long)| LatitudeLongitude::new(lat, long))
                .collect(),
        )
        .expect("at least two positions")
    }

    #[test]
    fn long_segments_get_densified_on_the_great_circle() {
        // About 714 km along the 50th parallel; the great circle bulges north of it by
        // about L² · tan(50°) / (8 · R), some 12 km or 0.107 degrees.
        let path = geometry(&[(50.0, 0.0), (50.0, 10.0)]);
        assert!(path.dense_positions.len() > 70);
        assert_eq!(path.positions.len(), 2);
        let middle = path.dense_positions[path.dense_positions.len() / 2];
        assert!((middle.latitude - 50.107).abs() < 0.002, "{middle:?}");
        // The bounds cover the bulge, not only the end points.
        assert!(path.bounds.min_max_lat.1 > 50.1);
        for pair in path.dense_positions.windows(2) {
            assert!(pair[0].get_distance_to(pair[1]) <= MAX_SEGMENT_METER * 1.01);
        }
        // The densified length is the great circle length of the original segment.
        let total = *path.distance_to_vertex.last().expect("not empty");
        let direct = path.positions[0].get_distance_to(path.positions[1]);
        assert!((total - direct).abs() < 1.0, "{total} vs {direct}");
    }

    #[test]
    fn short_segments_stay_as_they_are() {
        let path = geometry(&[(49.75, 6.63), (49.755, 6.64), (49.76, 6.645)]);
        assert_eq!(path.dense_positions, path.positions);
    }

    #[test]
    fn locate_measures_along_the_path() {
        // An L shape: east, then north.
        let path = geometry(&[(50.0, 7.0), (50.0, 7.01), (50.01, 7.01)]);
        let first_leg = path.positions[0].get_distance_to(path.positions[1]);

        // East of the second leg, halfway up: projects onto the second leg.
        let location = path.locate(LatitudeLongitude::new(50.005, 7.02));
        assert!((location.position.longitude - 7.01).abs() < 1e-9);
        let up = path.positions[1].get_distance_to(location.position);
        assert!((location.distance_from_start_meter - (first_leg + up)).abs() < 1e-6);

        // Before the start: clamps onto the first position.
        let location = path.locate(LatitudeLongitude::new(50.0, 6.9));
        // The round trip through Mercator costs nanometres.
        assert!(location.distance_from_start_meter.abs() < 1e-6);
        assert!((location.position.longitude - 7.0).abs() < 1e-9);
    }

    #[test]
    fn repeated_positions_do_not_break_the_projection() {
        let path = geometry(&[(50.0, 7.0), (50.0, 7.0), (50.0, 7.01)]);
        let location = path.locate(LatitudeLongitude::new(50.001, 7.005));
        assert!(location.distance_from_start_meter.is_finite());
        assert!((location.position.longitude - 7.005).abs() < 1e-9);
    }

    #[test]
    fn too_few_positions_give_no_path() {
        let mut system = PathSystem::default();
        assert_eq!(
            system.add_path(vec![], Color::BLACK, None),
            Err(PathError::TooFewPositions)
        );
        let key = system
            .add_path(
                vec![
                    LatitudeLongitude::new(50.0, 7.0),
                    LatitudeLongitude::new(50.0, 7.1),
                ],
                Color::BLACK,
                None,
            )
            .expect("two positions");
        assert_eq!(
            system.update_positions(key, vec![LatitudeLongitude::new(50.0, 7.0)]),
            Err(PathError::TooFewPositions)
        );
        assert_eq!(system.positions(key).map(<[_]>::len), Ok(2));
    }
}
