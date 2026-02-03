//! Material mapping from game textures to Brickadia materials.
//!
//! This module loads YAML configuration files that map texture names
//! to Brickadia materials (Plastic, Glass, Glow, Metallic, etc.).
//!
//! Directory structure: data/game_textures/<engine>/<game>/auto_2block_materials.yaml
//!
//! A cache file (material_cache.yaml) stores resolved texture->material mappings
//! for faster consecutive runs.

use crate::Material;
use bsp_converter::GameSource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A material category with prefixes and intensity.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MaterialCategory {
    #[serde(default)]
    pub intensity: Option<u8>,
    #[serde(default)]
    pub prefixes: Vec<String>,
}

/// Material mapping configuration loaded from YAML.
#[derive(Debug, Clone, Deserialize)]
pub struct MaterialMapping {
    /// Default material for unmatched textures
    #[serde(default = "default_material")]
    pub default: String,
    
    /// Default intensity for unmatched textures
    #[serde(default = "default_intensity")]
    pub default_intensity: u8,
    
    /// Glass textures
    #[serde(default)]
    pub glass: MaterialCategory,
    
    /// Metallic textures
    #[serde(default)]
    pub metallic: MaterialCategory,
    
    /// Glow textures
    #[serde(default)]
    pub glow: MaterialCategory,
    
    /// Hologram textures
    #[serde(default)]
    pub hologram: MaterialCategory,
    
    /// Ghost textures
    #[serde(default)]
    pub ghost: MaterialCategory,
    
    /// Plastic textures (explicit, otherwise default)
    #[serde(default)]
    pub plastic: MaterialCategory,
}

fn default_material() -> String {
    "Plastic".to_string()
}

fn default_intensity() -> u8 {
    5
}

impl Default for MaterialMapping {
    fn default() -> Self {
        Self {
            default: "Plastic".to_string(),
            default_intensity: 5,
            glass: MaterialCategory::default(),
            metallic: MaterialCategory::default(),
            glow: MaterialCategory::default(),
            hologram: MaterialCategory::default(),
            ghost: MaterialCategory::default(),
            plastic: MaterialCategory::default(),
        }
    }
}

impl MaterialMapping {
    /// Load material mapping from a YAML file.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read material mapping file: {}", e))?;
        
        serde_yaml::from_str(&content)
            .map_err(|e| format!("Failed to parse material mapping YAML: {}", e))
    }
    
    /// Check if a material mapping config exists for a game source.
    /// Returns the path if found, None otherwise.
    pub fn find_config_path(game_source: GameSource) -> Option<std::path::PathBuf> {
        if game_source == GameSource::Auto {
            return None;
        }
        
        let engine_folder = game_source.folder_name();
        let game_folder = game_source.game_name();
        
        // Try to find the config file in various locations
        // Structure: data/game_textures/<engine>/<game>/auto_2block_materials.yaml
        let possible_paths = [
            format!("data/game_textures/{}/{}/auto_2block_materials.yaml", engine_folder, game_folder),
            format!("./data/game_textures/{}/{}/auto_2block_materials.yaml", engine_folder, game_folder),
        ];
        
        for path_str in &possible_paths {
            let path = Path::new(path_str);
            if path.exists() {
                return Some(path.to_path_buf());
            }
        }
        
        None
    }
    
    /// Load material mapping for a specific game source.
    /// Looks in data/game_textures/<engine>/<game>/auto_2block_materials.yaml
    pub fn load_for_game(game_source: GameSource) -> Option<Self> {
        let path = Self::find_config_path(game_source)?;
        
        match Self::load_from_file(&path) {
            Ok(mapping) => Some(mapping),
            Err(_e) => None
        }
    }
    
    /// Get the Brickadia material and intensity for a texture name.
    /// Texture names are matched case-insensitively as prefixes.
    /// Returns (Material, intensity).
    pub fn get_material_and_intensity(&self, texture_name: &str) -> (Material, u8) {
        let texture_upper = texture_name.to_uppercase();
        
        // Check each material category
        for prefix in &self.glass.prefixes {
            if texture_upper.starts_with(&prefix.to_uppercase()) {
                return (Material::Glass, self.glass.intensity.unwrap_or(self.default_intensity));
            }
        }
        
        for prefix in &self.metallic.prefixes {
            if texture_upper.starts_with(&prefix.to_uppercase()) {
                return (Material::Metallic, self.metallic.intensity.unwrap_or(self.default_intensity));
            }
        }
        
        for prefix in &self.glow.prefixes {
            if texture_upper.starts_with(&prefix.to_uppercase()) {
                return (Material::Glow, self.glow.intensity.unwrap_or(self.default_intensity));
            }
        }
        
        for prefix in &self.hologram.prefixes {
            if texture_upper.starts_with(&prefix.to_uppercase()) {
                return (Material::Hologram, self.hologram.intensity.unwrap_or(self.default_intensity));
            }
        }
        
        for prefix in &self.ghost.prefixes {
            if texture_upper.starts_with(&prefix.to_uppercase()) {
                return (Material::Ghost, self.ghost.intensity.unwrap_or(self.default_intensity));
            }
        }
        
        for prefix in &self.plastic.prefixes {
            if texture_upper.starts_with(&prefix.to_uppercase()) {
                return (Material::Plastic, self.plastic.intensity.unwrap_or(self.default_intensity));
            }
        }
        
        // Return default material
        let default_mat = match self.default.to_lowercase().as_str() {
            "glass" => Material::Glass,
            "metallic" => Material::Metallic,
            "glow" => Material::Glow,
            "hologram" => Material::Hologram,
            "ghost" => Material::Ghost,
            _ => Material::Plastic,
        };
        
        (default_mat, self.default_intensity)
    }
    
    /// Get just the Brickadia material for a texture name (for backwards compatibility).
    #[allow(dead_code)]
    pub fn get_material(&self, texture_name: &str) -> Material {
        self.get_material_and_intensity(texture_name).0
    }
    
    /// Get the directory containing the config file for a game source.
    pub fn get_config_dir(game_source: GameSource) -> Option<PathBuf> {
        Self::find_config_path(game_source).map(|p| p.parent().unwrap().to_path_buf())
    }
    
    /// Get the assets directory for a game source (where extracted textures should be placed).
    /// Returns the path to data/game_textures/<engine>/<game>/assets/
    pub fn get_assets_dir(game_source: GameSource) -> Option<PathBuf> {
        Self::get_config_dir(game_source).map(|dir| dir.join("assets"))
    }
    
    /// Check if the assets directory exists and contains files.
    /// Returns (exists, has_files, path)
    pub fn check_assets_dir(game_source: GameSource) -> (bool, bool, Option<PathBuf>) {
        match Self::get_assets_dir(game_source) {
            Some(path) => {
                let exists = path.exists();
                let has_files = if exists {
                    std::fs::read_dir(&path)
                        .map(|mut entries| entries.next().is_some())
                        .unwrap_or(false)
                } else {
                    false
                };
                (exists, has_files, Some(path))
            }
            None => (false, false, None),
        }
    }
}

/// Cached material mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMaterial {
    pub material: String,
    pub intensity: u8,
}

/// Cache for resolved texture-to-material mappings.
/// Stored as material_cache.yaml in the game's config directory.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MaterialCache {
    /// Map of texture name -> cached material info
    #[serde(default)]
    pub textures: HashMap<String, CachedMaterial>,
}

impl MaterialCache {
    /// Get the cache file path for a game source.
    pub fn get_cache_path(game_source: GameSource) -> Option<PathBuf> {
        MaterialMapping::get_config_dir(game_source)
            .map(|dir| dir.join("material_cache.yaml"))
    }
    
    /// Load cache from file, or return empty cache if not found.
    pub fn load(game_source: GameSource) -> Self {
        let path = match Self::get_cache_path(game_source) {
            Some(p) => p,
            None => return Self::default(),
        };
        
        if !path.exists() {
            return Self::default();
        }
        
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                serde_yaml::from_str(&content).unwrap_or_default()
            }
            Err(_) => Self::default(),
        }
    }
    
    /// Save cache to file.
    pub fn save(&self, game_source: GameSource) -> Result<(), String> {
        let path = Self::get_cache_path(game_source)
            .ok_or_else(|| "No config directory for game source".to_string())?;
        
        let content = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize cache: {}", e))?;
        
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write cache file: {}", e))?;
        
        Ok(())
    }
    
    /// Get cached material for a texture, if available.
    pub fn get(&self, texture_name: &str) -> Option<(Material, u8)> {
        self.textures.get(texture_name).map(|cached| {
            let material = match cached.material.to_lowercase().as_str() {
                "glass" => Material::Glass,
                "metallic" => Material::Metallic,
                "glow" => Material::Glow,
                "hologram" => Material::Hologram,
                "ghost" => Material::Ghost,
                _ => Material::Plastic,
            };
            (material, cached.intensity)
        })
    }
    
    /// Add a texture mapping to the cache.
    pub fn insert(&mut self, texture_name: &str, material: Material, intensity: u8) {
        let material_str = match material {
            Material::Plastic => "Plastic",
            Material::Glass => "Glass",
            Material::Metallic => "Metallic",
            Material::Glow => "Glow",
            Material::Hologram => "Hologram",
            Material::Ghost => "Ghost",
        };
        
        self.textures.insert(
            texture_name.to_string(),
            CachedMaterial {
                material: material_str.to_string(),
                intensity,
            },
        );
    }
    
    /// Check if a texture is in the cache.
    #[allow(dead_code)]
    pub fn contains(&self, texture_name: &str) -> bool {
        self.textures.contains_key(texture_name)
    }
    
    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        self.textures.len()
    }
    
    /// Check if cache is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }
}

/// Material mapping with integrated cache support.
pub struct CachedMaterialMapping {
    pub mapping: MaterialMapping,
    pub cache: MaterialCache,
    pub game_source: GameSource,
    cache_modified: bool,
}

impl CachedMaterialMapping {
    /// Load material mapping and cache for a game source.
    pub fn load(game_source: GameSource) -> Option<Self> {
        let mapping = MaterialMapping::load_for_game(game_source)?;
        let cache = MaterialCache::load(game_source);
        
        Some(Self {
            mapping,
            cache,
            game_source,
            cache_modified: false,
        })
    }
    
    /// Pre-populate cache with all texture prefixes from the config.
    /// This allows the cache to be built ahead of time for faster lookups.
    pub fn build_cache_from_config(&mut self) {
        let categories = [
            (&self.mapping.glass.prefixes, Material::Glass, self.mapping.glass.intensity),
            (&self.mapping.metallic.prefixes, Material::Metallic, self.mapping.metallic.intensity),
            (&self.mapping.glow.prefixes, Material::Glow, self.mapping.glow.intensity),
            (&self.mapping.hologram.prefixes, Material::Hologram, self.mapping.hologram.intensity),
            (&self.mapping.ghost.prefixes, Material::Ghost, self.mapping.ghost.intensity),
            (&self.mapping.plastic.prefixes, Material::Plastic, self.mapping.plastic.intensity),
        ];
        
        for (prefixes, material, intensity_opt) in categories {
            let intensity = intensity_opt.unwrap_or(self.mapping.default_intensity);
            for prefix in prefixes {
                // Only add if not already in cache
                if !self.cache.contains(prefix) {
                    self.cache.insert(prefix, material, intensity);
                    self.cache_modified = true;
                }
            }
        }
    }
    
    /// Get material and intensity for a texture, using cache if available.
    pub fn get_material_and_intensity(&mut self, texture_name: &str) -> (Material, u8) {
        // Check cache first
        if let Some(result) = self.cache.get(texture_name) {
            return result;
        }
        
        // Not in cache, compute from mapping
        let (material, intensity) = self.mapping.get_material_and_intensity(texture_name);
        
        // Add to cache
        self.cache.insert(texture_name, material, intensity);
        self.cache_modified = true;
        
        (material, intensity)
    }
    
    /// Save the cache if it was modified.
    pub fn save_cache(&self) -> Result<(), String> {
        if self.cache_modified {
            self.cache.save(self.game_source)
        } else {
            Ok(())
        }
    }
    
    /// Get the number of cached entries.
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}
