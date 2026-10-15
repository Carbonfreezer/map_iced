//! This module contains the annotation system, that is in charge of administrating the [way points](waypoint_system),
//! the [paths](path_system) and the [regions](region_system).

pub(crate) mod annotation_compound;
pub mod annotation_support;
pub mod path_system;
pub mod region_system;
pub mod waypoint_system;
