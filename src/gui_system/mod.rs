//! Ths contains all high level functions that sit on top of the ui.
//! The central entry point is [`map_widget_system::MapWidgetSystem::boot`], which takes
//! a [`tile_cache_construction::TileCacheConfig`] describing where the tiles come from.

pub mod distance_units;
mod hashmap_stable;
pub(crate) mod high_level_tile_cache;
pub(crate) mod internal_math;
pub(crate) mod latitude_longitude;
pub(crate) mod map_widget;
pub mod map_widget_system;
pub mod tile_cache_construction;

pub use distance_units::DistanceUnits;
pub use latitude_longitude::LatitudeLongitude;
