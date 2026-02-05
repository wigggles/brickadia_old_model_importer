//! Conversion pipeline module
//!
//! This module organizes the OBJ/BSP to BRZ conversion logic into clear, separated concerns:
//!
//! - `pipeline`: Main conversion orchestration
//! - `material_processing`: Per-material statistics tracking
//! - `material_mapping`: Texture-to-Brickadia-material mapping
//! - `brick_alignment`: World coordinate restoration and alignment
//! - `grid_assembly`: Entity/grid creation for BRZ output
//!
//! ## Architecture
//!
//! ```text
//! main.rs
//!     │
//!     ▼
//! pipeline.rs  ──────────────────────────────┐
//!     │                                       │
//!     ├──▶ material_mapping.rs               │
//!     │                                       │
//!     ├──▶ material_processing.rs            │
//!     │        │                              │
//!     │        └──▶ voxel/voxelize.rs        │
//!     │                                       │
//!     ├──▶ brick_alignment.rs                │
//!     │                                       │
//!     └──▶ grid_assembly.rs                  │
//!              │                              │
//!              └──▶ output/brdb_support.rs ◀─┘
//! ```
//!
//! ## Key Design Principle
//!
//! **Octree sizing and world alignment are separate concerns.**
//!
//! - Octree sizing should ALWAYS use per-material bounds for efficiency
//! - World alignment is handled AFTER voxelization by adding bounds.min to brick positions

pub mod brick_alignment;
pub mod grid_assembly;
pub mod material_mapping;
pub mod material_processing;
pub mod pipeline;

pub use pipeline::perform_conversion;
