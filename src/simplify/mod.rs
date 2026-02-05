//! Brick simplification and generation
//!
//! This module contains algorithms for converting voxel data into optimized bricks:
//! - `grid`: Main simplification using VoxelGrid for O(1) access
//! - `direct`: Direct octree traversal for very large models (avoids memory overflow)

mod grid;
mod direct;

pub use grid::{simplify_lossy, simplify_lossless, simplify_lossy_with_material, simplify_lossless_with_material};
pub use direct::generate_bricks_direct;
