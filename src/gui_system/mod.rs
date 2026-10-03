//! Ths contains all high level functions that sit on top of the ui.
//! The central entry point of usage is here [`tile_cache_construction::TileCacheConfig`]
//! to generate the tile caching system.

mod direction_arrow;
mod focus_animation;
pub(crate) mod high_level_tile_cache;
pub(crate) mod internal_math;
pub mod latitude_longitude;
pub mod map_widget;
pub mod map_widget_system;
mod scale_bar;
pub mod tile_cache_construction;
pub (crate) mod map_widget_support;
