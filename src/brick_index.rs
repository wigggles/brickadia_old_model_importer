//! Brick Index System for Smart Brick Selection
//!
//! This module provides a registry of known Brickadia brick types and utilities
//! for intelligently selecting bricks based on context (position, neighbors, surface type).
//!
//! # Design Goals
//! 1. Keep the current default path (PB_DefaultBrick, PB_DefaultMicroBrick, PB_DefaultSmoothTile)
//! 2. Allow future expansion to use specialized bricks (ramps, corners, wedges)
//! 3. Support surface-aware brick selection (top surfaces get smooth tiles)
//! 4. Enable neighbor-aware selection for edge detection (future: auto-ramps)
//!
//! # Architecture
//! - `BrickRegistry`: Central registry of all known brick types from BRICK_TYPES.md
//! - `BrickSelector`: Strategy pattern for selecting appropriate bricks
//! - `BrickContext`: Information about a brick's position and neighbors
//!
//! # Usage
//! ```rust,ignore
//! let registry = BrickRegistry::default();
//! let selector = BrickSelector::new(&registry, SelectorMode::Default);
//! let brick_type = selector.select(&context);
//! ```

use std::collections::HashMap;

/// Brick category for organizing brick types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrickCategory {
    /// Basic rectangular bricks (PB_DefaultBrick, PB_DefaultMicroBrick)
    Basic,
    /// Flat tiles (PB_DefaultTile, PB_DefaultSmoothTile)
    Tile,
    /// Sloped surfaces (ramps, wedges)
    Slope,
    /// Corner pieces
    Corner,
    /// Round/cylindrical shapes
    Round,
    /// Decorative/special purpose
    Decorative,
}

/// Whether a brick is procedural (resizable) or fixed-size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrickSizing {
    /// Procedural brick - can be any size (PB_ prefix)
    Procedural,
    /// Fixed size brick - specific dimensions (B_ prefix)
    Fixed,
}

/// Information about a brick type from the registry
#[derive(Debug, Clone)]
pub struct BrickInfo {
    /// Asset name as used in brdb (e.g., "PB_DefaultBrick")
    pub asset_name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Category for organization
    pub category: BrickCategory,
    /// Whether this brick can be resized
    pub sizing: BrickSizing,
    /// Whether this brick has a sloped surface
    pub is_sloped: bool,
    /// Whether this brick has studs on top
    pub has_studs: bool,
}

/// Registry of all known Brickadia brick types
/// 
/// This is populated from BRICK_TYPES.md and provides lookup by name or category.
pub struct BrickRegistry {
    bricks: HashMap<&'static str, BrickInfo>,
}

impl Default for BrickRegistry {
    fn default() -> Self {
        let mut bricks = HashMap::new();
        
        // === Procedural Basic Shapes ===
        bricks.insert("PB_DefaultBrick", BrickInfo {
            asset_name: "PB_DefaultBrick",
            description: "Standard studded brick",
            category: BrickCategory::Basic,
            sizing: BrickSizing::Procedural,
            is_sloped: false,
            has_studs: true,
        });
        
        bricks.insert("PB_DefaultMicroBrick", BrickInfo {
            asset_name: "PB_DefaultMicroBrick",
            description: "Micro-scale studded brick (2x2x2 studs)",
            category: BrickCategory::Basic,
            sizing: BrickSizing::Procedural,
            is_sloped: false,
            has_studs: true,
        });
        
        bricks.insert("PB_DefaultTile", BrickInfo {
            asset_name: "PB_DefaultTile",
            description: "Flat tile with studs on top",
            category: BrickCategory::Tile,
            sizing: BrickSizing::Procedural,
            is_sloped: false,
            has_studs: true,
        });
        
        bricks.insert("PB_DefaultSmoothTile", BrickInfo {
            asset_name: "PB_DefaultSmoothTile",
            description: "Flat tile without studs (smooth surface)",
            category: BrickCategory::Tile,
            sizing: BrickSizing::Procedural,
            is_sloped: false,
            has_studs: false,
        });
        
        // === Procedural Ramps ===
        bricks.insert("PB_DefaultRamp", BrickInfo {
            asset_name: "PB_DefaultRamp",
            description: "Basic ramp (slope up along +Y)",
            category: BrickCategory::Slope,
            sizing: BrickSizing::Procedural,
            is_sloped: true,
            has_studs: false,
        });
        
        bricks.insert("PB_DefaultRampInverted", BrickInfo {
            asset_name: "PB_DefaultRampInverted",
            description: "Inverted ramp (slope down along +Y)",
            category: BrickCategory::Slope,
            sizing: BrickSizing::Procedural,
            is_sloped: true,
            has_studs: false,
        });
        
        bricks.insert("PB_DefaultRampCorner", BrickInfo {
            asset_name: "PB_DefaultRampCorner",
            description: "Outer corner ramp (diagonal)",
            category: BrickCategory::Corner,
            sizing: BrickSizing::Procedural,
            is_sloped: true,
            has_studs: false,
        });
        
        bricks.insert("PB_DefaultRampInnerCorner", BrickInfo {
            asset_name: "PB_DefaultRampInnerCorner",
            description: "Inner corner ramp (diagonal)",
            category: BrickCategory::Corner,
            sizing: BrickSizing::Procedural,
            is_sloped: true,
            has_studs: false,
        });
        
        // === Procedural Wedges (Micro-scale slopes) ===
        bricks.insert("PB_DefaultWedge", BrickInfo {
            asset_name: "PB_DefaultWedge",
            description: "Standard wedge (full-height slope)",
            category: BrickCategory::Slope,
            sizing: BrickSizing::Procedural,
            is_sloped: true,
            has_studs: false,
        });
        
        bricks.insert("PB_DefaultMicroWedge", BrickInfo {
            asset_name: "PB_DefaultMicroWedge",
            description: "Micro wedge (micro-scale slope)",
            category: BrickCategory::Slope,
            sizing: BrickSizing::Procedural,
            is_sloped: true,
            has_studs: false,
        });
        
        // === Procedural Poles ===
        bricks.insert("PB_DefaultPole", BrickInfo {
            asset_name: "PB_DefaultPole",
            description: "Cylindrical pole",
            category: BrickCategory::Round,
            sizing: BrickSizing::Procedural,
            is_sloped: false,
            has_studs: false,
        });
        
        Self { bricks }
    }
}

impl BrickRegistry {
    /// Get brick info by asset name
    pub fn get(&self, name: &str) -> Option<&BrickInfo> {
        self.bricks.get(name)
    }
    
    /// Get all bricks in a category
    pub fn by_category(&self, category: BrickCategory) -> Vec<&BrickInfo> {
        self.bricks.values()
            .filter(|b| b.category == category)
            .collect()
    }
    
    /// Get all procedural bricks
    pub fn procedural(&self) -> Vec<&BrickInfo> {
        self.bricks.values()
            .filter(|b| b.sizing == BrickSizing::Procedural)
            .collect()
    }
}

/// Context information for smart brick selection
#[derive(Debug, Clone)]
pub struct BrickContext {
    /// Position in voxel grid (x, y, z)
    pub position: (isize, isize, isize),
    /// Size of the merged brick region (width, depth, height)
    pub size: (isize, isize, isize),
    /// Whether this brick is at a top surface (no voxels above)
    pub is_top_surface: bool,
    /// Whether this brick is at a bottom surface (no voxels below)
    pub is_bottom_surface: bool,
    /// Neighbor information: (has_neighbor_x+, has_neighbor_x-, has_neighbor_y+, has_neighbor_y-, has_neighbor_z+, has_neighbor_z-)
    pub neighbors: [bool; 6],
}

impl BrickContext {
    /// Create a new context with default values
    pub fn new(position: (isize, isize, isize), size: (isize, isize, isize)) -> Self {
        Self {
            position,
            size,
            is_top_surface: false,
            is_bottom_surface: false,
            neighbors: [false; 6],
        }
    }
    
    /// Set top surface flag
    pub fn with_top_surface(mut self, is_top: bool) -> Self {
        self.is_top_surface = is_top;
        self
    }
}

/// Mode for brick selection strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorMode {
    /// Use only basic bricks (current default behavior)
    Default,
    /// Use smooth tiles for top surfaces only
    SmoothTops,
    /// Use ramps and wedges for sloped edges (future)
    SmartSlopes,
    /// Full smart selection with all brick types (future)
    Full,
}

/// Brick selector that chooses appropriate bricks based on context
pub struct BrickSelector<'a> {
    registry: &'a BrickRegistry,
    mode: SelectorMode,
    use_microbricks: bool,
}

impl<'a> BrickSelector<'a> {
    /// Create a new brick selector
    pub fn new(registry: &'a BrickRegistry, mode: SelectorMode) -> Self {
        Self {
            registry,
            mode,
            use_microbricks: false,
        }
    }
    
    /// Set whether to use microbricks
    pub fn with_microbricks(mut self, use_micro: bool) -> Self {
        self.use_microbricks = use_micro;
        self
    }
    
    /// Select the appropriate brick type for the given context
    pub fn select(&self, context: &BrickContext) -> &'static str {
        match self.mode {
            SelectorMode::Default => {
                if self.use_microbricks {
                    "PB_DefaultMicroBrick"
                } else {
                    "PB_DefaultBrick"
                }
            }
            SelectorMode::SmoothTops => {
                if self.use_microbricks {
                    "PB_DefaultMicroBrick"
                } else if context.is_top_surface {
                    "PB_DefaultSmoothTile"
                } else {
                    "PB_DefaultBrick"
                }
            }
            SelectorMode::SmartSlopes | SelectorMode::Full => {
                // Future: analyze neighbors to detect edges and use ramps/wedges
                // For now, fall back to SmoothTops behavior
                if self.use_microbricks {
                    "PB_DefaultMicroBrick"
                } else if context.is_top_surface {
                    "PB_DefaultSmoothTile"
                } else {
                    "PB_DefaultBrick"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_registry_default() {
        let registry = BrickRegistry::default();
        assert!(registry.get("PB_DefaultBrick").is_some());
        assert!(registry.get("PB_DefaultSmoothTile").is_some());
        assert!(registry.get("PB_DefaultMicroBrick").is_some());
    }
    
    #[test]
    fn test_selector_default_mode() {
        let registry = BrickRegistry::default();
        let selector = BrickSelector::new(&registry, SelectorMode::Default);
        let context = BrickContext::new((0, 0, 0), (1, 1, 1));
        
        assert_eq!(selector.select(&context), "PB_DefaultBrick");
    }
    
    #[test]
    fn test_selector_smooth_tops() {
        let registry = BrickRegistry::default();
        let selector = BrickSelector::new(&registry, SelectorMode::SmoothTops);
        
        let top_context = BrickContext::new((0, 0, 0), (1, 1, 1)).with_top_surface(true);
        let inner_context = BrickContext::new((0, 0, 0), (1, 1, 1)).with_top_surface(false);
        
        assert_eq!(selector.select(&top_context), "PB_DefaultSmoothTile");
        assert_eq!(selector.select(&inner_context), "PB_DefaultBrick");
    }
}
