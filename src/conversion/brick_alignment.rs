//! Brick alignment and world coordinate restoration
//!
//! This module handles the critical task of restoring brick positions to world coordinates
//! after voxelization. The key insight is that voxelization uses LOCAL coordinates for
//! efficiency (small octrees), but final bricks need WORLD coordinates for correct placement.
//!
//! ## Coordinate Flow
//!
//! ```text
//! World Coords (OBJ)     Local Coords (Octree)     World Coords (BRZ)
//! material at (200,50,300)  ->  voxels at (0,0,0)  ->  bricks at (200,50,300)
//!                          ^                       ^
//!                    subtract bounds.min      add bounds.min * scale
//! ```
//!
//! ## Why This Matters
//!
//! Without proper alignment:
//! - Each material would be placed at origin (0,0,0)
//! - Materials would overlap instead of forming the original model
//!
//! With proper alignment:
//! - Each material is placed at its original world position
//! - The complete model is reconstructed correctly

use cgmath::Vector3;

/// Configuration for brick alignment
#[derive(Debug, Clone)]
pub struct BrickAlignmentConfig {
    /// The brick scale factor (typically 2.0 * brick_scale)
    pub brick_scale_factor: f32,
    /// Global reference point for all materials (typically model's global_min)
    /// When None, each material uses its own bounds.min (correct behavior)
    #[allow(dead_code)]
    pub global_reference: Option<Vector3<f32>>,
}

impl Default for BrickAlignmentConfig {
    fn default() -> Self {
        Self {
            brick_scale_factor: 2.0,
            global_reference: None,
        }
    }
}

/// Result of aligning bricks to world coordinates
#[derive(Debug)]
pub struct AlignmentResult {
    /// Number of bricks aligned
    #[allow(dead_code)]
    pub brick_count: usize,
    /// Final position range after alignment
    pub position_range: PositionRange,
    /// Whether any corrections were applied (e.g., for negative positions)
    pub corrections_applied: bool,
}

/// Range of brick positions in each axis
#[derive(Debug, Default)]
pub struct PositionRange {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
    pub min_z: i32,
    pub max_z: i32,
}

impl PositionRange {
    pub fn from_bricks(bricks: &[brdb::Brick]) -> Self {
        if bricks.is_empty() {
            return Self::default();
        }
        
        let mut range = Self {
            min_x: i32::MAX,
            max_x: i32::MIN,
            min_y: i32::MAX,
            max_y: i32::MIN,
            min_z: i32::MAX,
            max_z: i32::MIN,
        };
        
        for brick in bricks {
            range.min_x = range.min_x.min(brick.position.x);
            range.max_x = range.max_x.max(brick.position.x);
            range.min_y = range.min_y.min(brick.position.y);
            range.max_y = range.max_y.max(brick.position.y);
            range.min_z = range.min_z.min(brick.position.z);
            range.max_z = range.max_z.max(brick.position.z);
        }
        
        range
    }
    
    pub fn has_negative(&self) -> bool {
        self.min_x < 0 || self.min_y < 0 || self.min_z < 0
    }
}

/// Align bricks from local octree coordinates to world coordinates.
///
/// ## Parameters
/// - `bricks`: Mutable slice of bricks to align (positions will be modified in place)
/// - `material_bounds_min`: The minimum bounds of the material (used as translation offset)
/// - `config`: Alignment configuration
///
/// ## How It Works
///
/// During voxelization, triangles are translated by `bounds.min` to create a small octree
/// starting near origin. This function reverses that translation:
///
/// ```text
/// final_position = local_position + (material_bounds_min * brick_scale_factor)
/// ```
///
/// This places each material's bricks at their correct world position.
pub fn align_bricks_to_world(
    bricks: &mut [brdb::Brick],
    material_bounds_min: Vector3<f32>,
    config: &BrickAlignmentConfig,
) -> AlignmentResult {
    if bricks.is_empty() {
        return AlignmentResult {
            brick_count: 0,
            position_range: PositionRange::default(),
            corrections_applied: false,
        };
    }
    
    // Calculate the offset to apply
    // This restores the material to its original world position
    let offset_x = (material_bounds_min.x * config.brick_scale_factor) as i32;
    let offset_y = (material_bounds_min.y * config.brick_scale_factor) as i32;
    let offset_z = (material_bounds_min.z * config.brick_scale_factor) as i32;
    
    // Apply world offset to all bricks
    for brick in bricks.iter_mut() {
        brick.position.x += offset_x;
        brick.position.y += offset_y;
        brick.position.z += offset_z;
    }
    
    // Calculate final position range
    let mut position_range = PositionRange::from_bricks(bricks);
    
    // Auto-correct negative positions if needed
    // Brickadia frozen grids don't render bricks at negative local positions correctly
    let corrections_applied = if position_range.has_negative() {
        let correction_x = if position_range.min_x < 0 { -position_range.min_x } else { 0 };
        let correction_y = if position_range.min_y < 0 { -position_range.min_y } else { 0 };
        let correction_z = if position_range.min_z < 0 { -position_range.min_z } else { 0 };
        
        for brick in bricks.iter_mut() {
            brick.position.x += correction_x;
            brick.position.y += correction_y;
            brick.position.z += correction_z;
        }
        
        // Update range after correction
        position_range.min_x += correction_x;
        position_range.max_x += correction_x;
        position_range.min_y += correction_y;
        position_range.max_y += correction_y;
        position_range.min_z += correction_z;
        position_range.max_z += correction_z;
        
        true
    } else {
        false
    };
    
    AlignmentResult {
        brick_count: bricks.len(),
        position_range,
        corrections_applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_alignment_basic() {
        // Test that alignment correctly offsets bricks
        let mut bricks = vec![
            brdb::Brick {
                position: brdb::Vector3i { x: 0, y: 0, z: 0 },
                ..Default::default()
            },
            brdb::Brick {
                position: brdb::Vector3i { x: 10, y: 10, z: 10 },
                ..Default::default()
            },
        ];
        
        let bounds_min = Vector3::new(100.0, 50.0, 200.0);
        let config = BrickAlignmentConfig {
            brick_scale_factor: 2.0,
            global_reference: None,
        };
        
        let result = align_bricks_to_world(&mut bricks, bounds_min, &config);
        
        // Bricks should be offset by bounds_min * 2.0
        assert_eq!(bricks[0].position.x, 200); // 0 + 100*2
        assert_eq!(bricks[0].position.y, 100); // 0 + 50*2
        assert_eq!(bricks[0].position.z, 400); // 0 + 200*2
        
        assert_eq!(bricks[1].position.x, 210); // 10 + 100*2
        assert_eq!(bricks[1].position.y, 110); // 10 + 50*2
        assert_eq!(bricks[1].position.z, 410); // 10 + 200*2
        
        assert_eq!(result.brick_count, 2);
        assert!(!result.corrections_applied);
    }
}
