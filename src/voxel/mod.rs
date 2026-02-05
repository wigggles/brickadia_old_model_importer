//! Voxelization module
//!
//! This module contains the core voxelization algorithms and data structures:
//!
//! - `octree`: Sparse octree data structure for efficient voxel storage
//! - `voxelize`: Triangle-to-voxel conversion algorithms

mod octree;
mod voxelize;

pub use octree::*;
pub use voxelize::*;
