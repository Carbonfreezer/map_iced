//! Ths contains all high level functions that sit on top of the ui.
//! The central entry point of usage is here [`tile_cache_construction::TileCacheConfig`]
//! to generate the tile caching system.

mod hashmap_stable;
pub(crate) mod high_level_tile_cache;
pub(crate) mod internal_math;
pub(crate) mod latitude_longitude;
pub(crate) mod map_widget;
pub mod map_widget_system;
pub mod tile_cache_construction;

pub use latitude_longitude::LatitudeLongitude;
