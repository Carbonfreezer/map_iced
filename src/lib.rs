//! This module implements an iced widget for displaying maps and map annotations.
//! Currently supported as annotations are way points and paths.
//! It contains a tile caching system
//! and can operate with different tile providers that works with the  [slippy map convention](https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames).

#![warn(clippy::await_holding_lock)]

/// Re-exported so that callers of [`WaypointSymbol::Image`] do not have to depend on
/// `bytes` themselves and keep its version in step with ours.
///
/// [`WaypointSymbol::Image`]: annotation_system::waypoint_system::WaypointSymbol::Image
pub use bytes::Bytes;

pub mod annotation_system;
pub mod gui_system;
pub(crate) mod tile_cache;
