//! Ths contains all high level functions that sit on top of the ui.
//! The central entry point of usage is here [`tile_cache_construction::TileCacheConfig`]
//! to generate the tile caching system.

pub(crate) mod high_level_tile_cache;
pub(crate) mod internal_math;
pub mod latitude_longitude;
pub mod map_widget;
pub mod map_widget_system;
pub mod tile_cache_construction;
mod hashmap_stable;
