//! Material processing statistics for split-by-material conversion
//!
//! This module provides statistics tracking for the per-material conversion loop.
//!
//! ## Key Design Decisions
//!
//! 1. **Octree sizing is ALWAYS per-material**: This ensures small, efficient octrees
//!    regardless of where the material is located in world space.
//!
//! 2. **World alignment is handled AFTER voxelization**: The `brick_alignment` module
//!    restores bricks to world coordinates using the material's bounds.min.
//!
//! 3. **These concerns are SEPARATE**: Changing alignment should never affect octree size.

/// Statistics for a batch of material processing
#[derive(Debug, Default)]
pub struct MaterialBatchStats {
    /// Total materials processed
    pub processed_count: usize,
    /// Total materials skipped
    pub skipped_count: usize,
    /// Total voxels extracted
    #[allow(dead_code)]
    pub total_voxels: usize,
    /// Total bricks generated
    pub total_bricks: usize,
    /// Total processing time
    pub total_time_secs: f32,
    /// Largest octree size encountered
    pub max_octree_size: u8,
    /// Number of materials with octree size >= 9 (slow)
    pub large_octree_count: usize,
}

impl MaterialBatchStats {
    /// Record a skipped material
    pub fn record_skipped(&mut self) {
        self.skipped_count += 1;
    }
    
    /// Record a processed material with its stats
    pub fn record_processed(&mut self, brick_count: usize, octree_size: u8, time_secs: f32) {
        self.processed_count += 1;
        self.total_bricks += brick_count;
        self.total_time_secs += time_secs;
        self.max_octree_size = self.max_octree_size.max(octree_size);
        if octree_size >= 9 {
            self.large_octree_count += 1;
        }
    }
    
    pub fn average_time_per_material(&self) -> f32 {
        if self.processed_count == 0 {
            1.0 // Default assumption
        } else {
            self.total_time_secs / self.processed_count as f32
        }
    }
}
