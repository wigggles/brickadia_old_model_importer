//! Color utilities for voxelization and brick generation
//!
//! This module contains:
//! - `utils`: Color conversion functions (RGB/HSV, float/int)
//! - `palette`: Default Brickadia color palette
//! - `merge`: Color merging for brick optimization

pub mod utils;
pub mod palette;
pub mod merge;

pub use utils::*;
