//! Grid assembly for BRZ output
//!
//! This module handles the creation of Entity/Grid structures for the final BRZ file.
//! It supports both per-material grids and grouped-by-brick-material grids.
//!
//! ## Grid Assembly Modes
//!
//! 1. **Per-texture-material**: One grid per OBJ material (default)
//!    - Preserves material separation for editing
//!    - Each material can be independently manipulated in Brickadia
//!
//! 2. **Grouped-by-brick-material**: One grid per Brickadia material type
//!    - Consolidates all Plastic bricks into one grid, all Metallic into another, etc.
//!    - Reduces grid count but loses texture material separation

use brdb::{Brick, Entity, Vector3f};
use crate::app::Material;
use std::collections::HashMap;

/// Configuration for grid assembly
#[derive(Debug, Clone)]
pub struct GridAssemblyConfig {
    /// Offset between grids in X direction
    pub grid_offset_x: f32,
    /// Offset between grids in Y direction
    pub grid_offset_y: f32,
    /// Offset between grids in Z direction
    pub grid_offset_z: f32,
    /// Whether to group by Brickadia material type
    pub group_by_brick_material: bool,
}

impl Default for GridAssemblyConfig {
    fn default() -> Self {
        Self {
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_offset_z: 0.0,
            group_by_brick_material: false,
        }
    }
}

impl GridAssemblyConfig {
    /// Check if any grid offset is set
    pub fn has_offset(&self) -> bool {
        self.grid_offset_x != 0.0 
            || self.grid_offset_y != 0.0 
            || self.grid_offset_z != 0.0
    }
    
    /// Calculate the entity location for a given index
    pub fn entity_location(&self, index: usize) -> Vector3f {
        let multiplier = if self.has_offset() { index as f32 } else { 0.0 };
        Vector3f {
            x: self.grid_offset_x * multiplier,
            y: self.grid_offset_y * multiplier,
            z: self.grid_offset_z * multiplier,
        }
    }
}

/// Input for grid assembly: bricks from a single texture material
pub struct MaterialBricks {
    #[allow(dead_code)]
    pub material_id: usize,
    #[allow(dead_code)]
    pub material_name: String,
    pub bricks: Vec<Brick>,
    pub brick_material: Material,
}

/// Result of grid assembly
pub struct GridAssemblyResult {
    /// Assembled grids ready for BRZ output
    pub grids: Vec<(Entity, Vec<Brick>)>,
    /// Number of grids created
    pub grid_count: usize,
    /// Total bricks across all grids
    #[allow(dead_code)]
    pub total_bricks: usize,
}

/// Assemble bricks into grids for BRZ output
///
/// ## Parameters
/// - `material_bricks`: Vec of MaterialBricks containing bricks grouped by texture material
/// - `config`: Grid assembly configuration
///
/// ## Returns
/// GridAssemblyResult with grids ready for BRZ writing
pub fn assemble_grids(
    material_bricks: Vec<MaterialBricks>,
    config: &GridAssemblyConfig,
) -> GridAssemblyResult {
    let grids = if config.group_by_brick_material {
        assemble_by_brick_material(material_bricks, config)
    } else {
        assemble_by_texture_material(material_bricks, config)
    };
    
    let total_bricks = grids.iter().map(|(_, b)| b.len()).sum();
    let grid_count = grids.len();
    
    GridAssemblyResult {
        grids,
        grid_count,
        total_bricks,
    }
}

/// Assemble grids grouped by Brickadia material type
///
/// All bricks with the same Brickadia material (Plastic, Metallic, etc.) are
/// combined into a single grid, regardless of their original texture material.
fn assemble_by_brick_material(
    material_bricks: Vec<MaterialBricks>,
    config: &GridAssemblyConfig,
) -> Vec<(Entity, Vec<Brick>)> {
    // Group bricks by their Brickadia material
    let mut by_material: HashMap<Material, Vec<Brick>> = HashMap::new();
    
    for mb in material_bricks {
        by_material
            .entry(mb.brick_material)
            .or_default()
            .extend(mb.bricks);
    }
    
    // Create one grid per Brickadia material
    let mut grids = Vec::new();
    
    for (index, (_brick_material, bricks)) in by_material.into_iter().enumerate() {
        let bricks: Vec<Brick> = bricks;
        if bricks.is_empty() {
            continue;
        }
        
        let entity = Entity {
            frozen: false,
            location: config.entity_location(index),
            ..Default::default()
        };
        grids.push((entity, bricks));
    }
    
    grids
}

/// Assemble grids with one grid per texture material
///
/// Each OBJ/texture material gets its own grid. This preserves the material
/// separation from the original model.
fn assemble_by_texture_material(
    material_bricks: Vec<MaterialBricks>,
    config: &GridAssemblyConfig,
) -> Vec<(Entity, Vec<Brick>)> {
    material_bricks
        .into_iter()
        .filter(|mb| !mb.bricks.is_empty())
        .enumerate()
        .map(|(index, mb)| {
            let entity = Entity {
                frozen: false,
                location: config.entity_location(index),
                ..Default::default()
            };
            (entity, mb.bricks)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use brdb::Vector3i;
    
    fn make_test_brick(x: i32, y: i32, z: i32) -> Brick {
        Brick {
            position: Vector3i { x, y, z },
            ..Default::default()
        }
    }
    
    #[test]
    fn test_assemble_by_texture_material_no_offset() {
        let material_bricks = vec![
            MaterialBricks {
                material_id: 0,
                material_name: "mat0".to_string(),
                bricks: vec![make_test_brick(0, 0, 0)],
                brick_material: Material::Plastic,
            },
            MaterialBricks {
                material_id: 1,
                material_name: "mat1".to_string(),
                bricks: vec![make_test_brick(10, 10, 10)],
                brick_material: Material::Metallic,
            },
        ];
        
        let config = GridAssemblyConfig::default();
        let result = assemble_grids(material_bricks, &config);
        
        assert_eq!(result.grid_count, 2);
        assert_eq!(result.total_bricks, 2);
        
        // Both grids should be at origin (no offset)
        assert_eq!(result.grids[0].0.location.x, 0.0);
        assert_eq!(result.grids[1].0.location.x, 0.0);
    }
    
    #[test]
    fn test_assemble_by_texture_material_with_offset() {
        let material_bricks = vec![
            MaterialBricks {
                material_id: 0,
                material_name: "mat0".to_string(),
                bricks: vec![make_test_brick(0, 0, 0)],
                brick_material: Material::Plastic,
            },
            MaterialBricks {
                material_id: 1,
                material_name: "mat1".to_string(),
                bricks: vec![make_test_brick(10, 10, 10)],
                brick_material: Material::Metallic,
            },
        ];
        
        let config = GridAssemblyConfig {
            grid_offset_x: 100.0,
            grid_offset_y: 0.0,
            grid_offset_z: 0.0,
            group_by_brick_material: false,
        };
        let result = assemble_grids(material_bricks, &config);
        
        assert_eq!(result.grid_count, 2);
        
        // First grid at origin, second offset by 100
        assert_eq!(result.grids[0].0.location.x, 0.0);
        assert_eq!(result.grids[1].0.location.x, 100.0);
    }
    
    #[test]
    fn test_assemble_by_brick_material() {
        let material_bricks = vec![
            MaterialBricks {
                material_id: 0,
                material_name: "mat0".to_string(),
                bricks: vec![make_test_brick(0, 0, 0)],
                brick_material: Material::Plastic,
            },
            MaterialBricks {
                material_id: 1,
                material_name: "mat1".to_string(),
                bricks: vec![make_test_brick(10, 10, 10)],
                brick_material: Material::Plastic, // Same as mat0
            },
            MaterialBricks {
                material_id: 2,
                material_name: "mat2".to_string(),
                bricks: vec![make_test_brick(20, 20, 20)],
                brick_material: Material::Metallic,
            },
        ];
        
        let config = GridAssemblyConfig {
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_offset_z: 0.0,
            group_by_brick_material: true,
        };
        let result = assemble_grids(material_bricks, &config);
        
        // Should consolidate into 2 grids: Plastic (2 bricks) and Metallic (1 brick)
        assert_eq!(result.grid_count, 2);
        assert_eq!(result.total_bricks, 3);
    }
    
    #[test]
    fn test_empty_materials_filtered() {
        let material_bricks = vec![
            MaterialBricks {
                material_id: 0,
                material_name: "mat0".to_string(),
                bricks: vec![make_test_brick(0, 0, 0)],
                brick_material: Material::Plastic,
            },
            MaterialBricks {
                material_id: 1,
                material_name: "mat1".to_string(),
                bricks: vec![], // Empty
                brick_material: Material::Metallic,
            },
        ];
        
        let config = GridAssemblyConfig::default();
        let result = assemble_grids(material_bricks, &config);
        
        // Empty material should be filtered out
        assert_eq!(result.grid_count, 1);
        assert_eq!(result.total_bricks, 1);
    }
}
