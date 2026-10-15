//! This module implements an iced widget for displaying maps and map annotations.
//! Supported annotations are way points, paths and regions.
//! It contains a tile caching system
//! and can operate with different tile providers that works with the  [slippy map convention](https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames).

#![warn(clippy::await_holding_lock)]

pub mod annotation_system;
pub mod gui_system;
pub(crate) mod tile_cache;
