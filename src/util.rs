//! Utility functions used across the application.
//!
//! Contains helper functions for file operations, formatting, and other common tasks.

use std::path::Path;

use crate::color;
use crate::error::{ConversionError, ConversionResult, MissingResources};
use crate::app::Logger;
use tobj::LoadOptions;

/// Enable verbose debug logging throughout the conversion pipeline.
/// Set to `true` to see detailed information about each step.
pub const DEBUG_MODE: bool = true;

/// Enable verbose model loading logs (model names, vertex/face counts per model).
/// When false, only shows total model count summary.
pub const VERBOSE_MODEL_LOADING: bool = false;

/// Enable verbose texture loading logs (shows each texture being loaded).
/// When false, only shows materials that are missing textures.
pub const VERBOSE_TEXTURE_LOADING: bool = false;

/// Opens a folder in the system file explorer.
///
/// Cross-platform function that opens the specified folder path in:
/// - Windows: File Explorer
/// - macOS: Finder
/// - Linux: Default file manager
pub fn open_folder_in_explorer(path: &str) -> std::io::Result<()> {
    let path = Path::new(path);
    
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Path does not exist",
        ));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()?;
    }

    Ok(())
}

/// Creates a 1x1 solid color texture from material color
pub fn create_solid_color_texture(diffuse: [f32; 3], dissolve: f32) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(1, 1);
    img.put_pixel(
        0,
        0,
        image::Rgba([
            color::ftoi(diffuse[0]),
            color::ftoi(diffuse[1]),
            color::ftoi(diffuse[2]),
            color::ftoi(dissolve),
        ]),
    );
    img
}

/// Validates OBJ file and checks for missing resources
pub fn validate_obj_resources(obj_path: &str) -> ConversionResult<MissingResources> {
    let p = Path::new(obj_path);

    // Check if OBJ file exists
    if !p.exists() {
        return Err(ConversionError::ObjFileNotFound { path: p.to_path_buf() });
    }

    let load_options = LoadOptions {
        triangulate: true,
        ignore_lines: true,
        ignore_points: true,
        single_index: true,
    };

    let (_models, materials) = tobj::load_obj(obj_path, &load_options)
        .map_err(|e| ConversionError::ObjParseError(e.to_string()))?;

    let mut missing = MissingResources::new();

    // Check if materials exist
    let materials = match materials {
        Ok(mats) if !mats.is_empty() => mats,
        Ok(_) | Err(_) => {
            missing.missing_materials = true;
            return Ok(missing);
        }
    };

    // Check each material for missing textures
    for material in materials {
        if let Some(texture_name) = &material.diffuse_texture {
            if !texture_name.is_empty() {
                let texture_path = p.parent()
                    .ok_or_else(|| ConversionError::ObjFileNotFound { path: p.to_path_buf() })?
                    .join(texture_name);

                if !texture_path.exists() {
                    missing.missing_textures.push((material.name.clone(), texture_path));
                }
            }
        }
    }

    Ok(missing)
}

/// Format a number with comma separators for readability (e.g., 1234567 -> "1,234,567")
pub fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Log a debug message only when DEBUG_MODE is enabled
pub fn debug_log(logger: &Logger, message: String) {
    if DEBUG_MODE {
        logger.log(format!("[DEBUG] {}", message));
    }
}
