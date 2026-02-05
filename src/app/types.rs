//! Application types and data structures.
//!
//! Contains the main `Obj2Brz` application struct and related enums/types.

use bsp_converter::GameSource;
use brdb::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use super::logger::Logger;

/// Intermediate data structure for building the save
#[derive(Clone)]
pub struct SaveData {
    pub bricks: Vec<brdb::Brick>,
    pub colors: Vec<Color>,
    pub author_name: String,
}

/// The type of input file being processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputFileType {
    /// Wavefront OBJ file (direct voxelization).
    Obj,
    /// BSP map file (requires conversion to OBJ first).
    Bsp,
}

impl Default for InputFileType {
    fn default() -> Self {
        InputFileType::Obj
    }
}

/// How to apply colors to bricks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorMode {
    /// Use texture colors from the model.
    TextureColors,
    /// Use a single solid color for all bricks (based on Material setting).
    SingleColor,
    /// Use material diffuse color from OBJ/MTL file.
    MaterialColor,
}

impl Default for ColorMode {
    fn default() -> Self {
        ColorMode::TextureColors
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum BrickType {
    Microbricks,
    Default,
    Tiles,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum Material {
    Plastic,
    Glass,
    Glow,
    Metallic,
    Hologram,
    Ghost,
}

/// A settings profile containing conversion settings (not paths).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsProfile {
    pub name: String,
    pub bricktype: BrickType,
    pub brick_scale: isize,
    pub material: Material,
    pub material_intensity: u32,
    pub scale: f32,
    pub simplify: bool,
    pub match_brickadia_colorset: bool,
    #[serde(default)]
    pub color_mode: ColorMode,
    pub rotation_x: i32,
    pub rotation_y: i32,
    pub rotation_z: i32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_z: f32,
    pub show_origin_marker: bool,
}

impl Default for SettingsProfile {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            bricktype: BrickType::Microbricks,
            brick_scale: 1,
            material: Material::Plastic,
            material_intensity: 5,
            scale: 1.0,
            simplify: false,
            match_brickadia_colorset: false,
            color_mode: ColorMode::TextureColors,
            rotation_x: 0,
            rotation_y: 0,
            rotation_z: 0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            show_origin_marker: false,
        }
    }
}

fn default_axis_scale() -> f32 {
    1.0
}

fn default_single_color() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_save_description() -> String {
    "Converted with obj2brz".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Obj2Brz {
    pub bricktype: BrickType,
    pub brick_scale: isize,
    #[serde(skip)]
    pub input_file_path_receiver: Option<Receiver<Option<PathBuf>>>,
    pub input_file_path: String,
    pub match_brickadia_colorset: bool,
    #[serde(default)]
    pub color_mode: ColorMode,
    pub material: Material,
    pub material_intensity: u32,
    #[serde(skip)]
    pub output_directory_receiver: Option<Receiver<Option<PathBuf>>>,
    pub output_directory: String,
    pub save_owner_id: String,
    pub save_owner_name: String,
    pub save_name: String,
    pub scale: f32,
    pub simplify: bool,
    pub split_by_material: bool,
    pub grid_offset_x: f32,
    pub grid_offset_y: f32,
    pub grid_offset_z: f32,
    /// Rotation around X axis in 90 increments (0, 90, 180, 270)
    #[serde(default)]
    pub rotation_x: i32,
    /// Rotation around Y axis in 90 increments (0, 90, 180, 270)
    #[serde(default)]
    pub rotation_y: i32,
    /// Rotation around Z axis in 90 increments (0, 90, 180, 270)
    #[serde(default)]
    pub rotation_z: i32,
    /// Add origin marker with color-coded X/Y/Z axis bricks
    #[serde(default)]
    pub show_origin_marker: bool,
    /// Scale multiplier for X axis (default 1.0)
    #[serde(default = "default_axis_scale")]
    pub scale_x: f32,
    /// Scale multiplier for Y axis (default 1.0)
    #[serde(default = "default_axis_scale")]
    pub scale_y: f32,
    /// Scale multiplier for Z axis (default 1.0) - adjust to fix squished height
    #[serde(default = "default_axis_scale")]
    pub scale_z: f32,
    /// Use texture-based material mapping for BSP conversions
    #[serde(default)]
    pub use_material_mapping: bool,
    /// Group output grids by Brickadia material type (Plastic, Glass, Glow, etc.)
    /// instead of one grid per texture. Can reduce 300+ grids to ~6 grids in some instances.
    #[serde(default)]
    pub group_by_brick_material: bool,
    /// Use smooth bricks without studs for a polished finish
    #[serde(default)]
    pub use_smooth_bricks: bool,
    /// Color merge threshold for combining similar colored blocks (0.0 = off, 0.1-1.0 = sensitivity)
    #[serde(default)]
    pub color_merge_threshold: f32,
    /// RGB color for Single Color mode (0.0-1.0 range)
    #[serde(default = "default_single_color")]
    pub single_color: [f32; 3],
    /// Description text embedded in .brz file
    #[serde(default = "default_save_description")]
    pub save_description: String,
    #[serde(skip)]
    pub missing_resources_dialog: Option<String>,
    #[serde(skip)]
    pub pending_conversion_skip_textures: bool,
    #[serde(skip)]
    pub logger: Logger,
    #[serde(skip)]
    pub conversion_in_progress: bool,
    #[serde(skip)]
    pub conversion_done_receiver: Option<Receiver<()>>,
    #[serde(skip)]
    pub conversion_progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    #[serde(skip)]
    pub conversion_stage: std::sync::Arc<std::sync::Mutex<String>>,
    #[serde(skip)]
    pub conversion_cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[serde(skip)]
    pub conversion_start_time: Option<std::time::Instant>,

    // BSP conversion options
    /// Detected or selected input file type.
    #[serde(default)]
    pub input_file_type: InputFileType,
    /// Selected game source for BSP conversion.
    #[serde(default)]
    pub bsp_game_source: GameSource,
    /// Detected game source (for display, may differ from selected).
    #[serde(skip)]
    pub detected_game_source: Option<GameSource>,
    
    // Settings profiles
    /// Saved settings profiles
    #[serde(default)]
    pub profiles: Vec<SettingsProfile>,
    /// Currently selected profile index (None = custom/unsaved)
    #[serde(skip)]
    pub selected_profile_index: Option<usize>,
    /// Name for new profile when saving
    #[serde(skip)]
    pub new_profile_name: String,
    
    // Help button expanded states
    #[serde(skip)]
    pub help_lossy_expanded: bool,
    #[serde(skip)]
    pub help_color_merge_expanded: bool,
    #[serde(skip)]
    pub help_scale_expanded: bool,
    #[serde(skip)]
    pub help_bricktype_expanded: bool,
    #[serde(skip)]
    pub help_color_mode_expanded: bool,
    #[serde(skip)]
    pub help_surface_expanded: bool,
}

impl Default for Obj2Brz {
    fn default() -> Self {
        // Get auto-suggested paths from data directory
        let imports_dir = super::logger::get_imports_dir();
        let exports_dir = super::logger::get_exports_dir();

        // Use data/imports as default input path hint, data/exports as output
        let default_input = if imports_dir.exists() {
            imports_dir.to_string_lossy().to_string()
        } else {
            "".into()
        };

        let default_output = exports_dir.to_string_lossy().to_string();

        Self {
            bricktype: BrickType::Microbricks,
            brick_scale: 1,
            input_file_path_receiver: None,
            input_file_path: default_input,
            match_brickadia_colorset: false,
            color_mode: ColorMode::TextureColors,
            material: Material::Plastic,
            material_intensity: 5,
            output_directory_receiver: None,
            output_directory: default_output,
            save_owner_id: "d66c4ad5-59fc-4a9b-80b8-08dedc25bff9".into(),
            save_owner_name: "obj2brz".into(),
            save_name: "converted".into(),
            scale: 1.0,
            simplify: false,
            split_by_material: false,
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_offset_z: 0.0,
            rotation_x: 0,
            rotation_y: 0,
            rotation_z: 0,
            show_origin_marker: false,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            use_material_mapping: false,
            group_by_brick_material: false,
            use_smooth_bricks: false,
            color_merge_threshold: 0.0,
            single_color: [1.0, 1.0, 1.0],
            save_description: "Converted with obj2brz".to_string(),
            missing_resources_dialog: None,
            pending_conversion_skip_textures: false,
            logger: Logger::default(),
            conversion_in_progress: false,
            conversion_done_receiver: None,
            conversion_progress: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            conversion_stage: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
            conversion_cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            conversion_start_time: None,
            // BSP options
            input_file_type: InputFileType::Obj,
            bsp_game_source: GameSource::Auto,
            detected_game_source: None,
            // Profiles
            profiles: Vec::new(),
            selected_profile_index: None,
            new_profile_name: String::new(),
            // Help button states
            help_lossy_expanded: false,
            help_color_merge_expanded: false,
            help_scale_expanded: false,
            help_bricktype_expanded: false,
            help_color_mode_expanded: false,
            help_surface_expanded: false,
        }
    }
}
