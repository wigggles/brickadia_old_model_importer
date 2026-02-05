//! Geometry utilities for voxelization
//!
//! This module contains mathematical functions for:
//! - Triangle-box intersection testing (`intersect`)
//! - Barycentric coordinate interpolation (`barycentric`)

pub mod barycentric;
pub mod intersect;

pub use barycentric::interpolate_uv;
pub use intersect::intersect;
