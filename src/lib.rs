//! This module implements an iced widget for displaying maps and map annotations. It contains a tile caching system
//! and can operate with different tile providers that works with the  [slippy map convention](https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames).

#![warn(clippy::await_holding_lock)]

pub mod gui_system;
pub(crate) mod tile_cache;
