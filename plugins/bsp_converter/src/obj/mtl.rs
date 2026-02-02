//! # MTL (Material) File Writer
//!
//! This module writes Wavefront MTL files from BSP material data.
//!
//! ## MTL Format Overview
//!
//! MTL is a companion format to OBJ that defines materials:
//! - `newmtl name` — Define a new material
//! - `Ka r g b` — Ambient color
//! - `Kd r g b` — Diffuse color
//! - `Ks r g b` — Specular color
//! - `Ns value` — Specular exponent
//! - `d value` — Dissolve (opacity)
//! - `illum model` — Illumination model
//! - `map_Kd path` — Diffuse texture map
//!
//! ## Texture paths
//!
//! The `map_Kd` paths are relative to the OBJ file location.
//! BSP materials typically reference textures like `textures/common/brick`.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::bsp::BspData;
use crate::BspResult;

// -----------------------------------------------------------------------------
// MTL Writer
// -----------------------------------------------------------------------------

/// Write BSP materials to an MTL file.
///
/// Creates a material library file that the OBJ file references.
///
/// # Arguments
///
/// * `data` - The parsed BSP data containing material names.
/// * `output_dir` - Directory where the MTL file will be written.
///
/// # Returns
///
/// `Ok(())` on success, or an IO error.
///
/// # Example
///
/// ```ignore
/// let data = cod1::parse("maps/mp_harbor.bsp")?;
/// write_mtl(&data, "output/")?;
/// // Creates: output/mp_harbor.mtl
/// ```
pub fn write_mtl<P: AsRef<Path>>(data: &BspData, output_dir: P) -> BspResult<()> {
    let output_dir = output_dir.as_ref();

    // Create output directory if needed
    fs::create_dir_all(output_dir)?;

    // Create textures subdirectory
    let textures_dir = output_dir.join("textures");
    fs::create_dir_all(&textures_dir)?;

    // Build texture lookup map (name -> texture data)
    let texture_map: HashMap<&str, &crate::bsp::TextureData> = data
        .textures
        .iter()
        .map(|t| (t.name.as_str(), t))
        .collect();

    // Export embedded textures as PNG files
    for texture in &data.textures {
        if texture.pixels.is_empty() {
            continue; // Skip external textures (no pixel data)
        }

        // Sanitize texture name for filename (replace special chars)
        let safe_name = texture.name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let png_path = textures_dir.join(format!("{}.png", safe_name));

        // Write PNG file
        if let Err(e) = write_texture_png(texture, &png_path) {
            eprintln!("Warning: Failed to write texture '{}': {}", texture.name, e);
        }
    }

    // Build output path
    let mtl_path = output_dir.join(format!("{}.mtl", data.map_name));

    // Open file for writing
    let file = File::create(&mtl_path)?;
    let mut writer = BufWriter::new(file);

    // Write header comment
    writeln!(writer, "# MTL file exported by bsp_converter")?;
    writeln!(writer, "# Map: {}", data.map_name)?;
    writeln!(writer, "# Materials: {}", data.materials.len())?;
    writeln!(writer)?;

    // Write each material
    for material_name in &data.materials {
        // Material name (used by OBJ's usemtl directive)
        writeln!(writer, "newmtl {}", material_name)?;

        // Check if we have texture data for this material
        let texture_opt = texture_map.get(material_name.as_str());
        let has_texture = texture_opt
            .map(|t| !t.pixels.is_empty())
            .unwrap_or(false);

        // Compute diffuse color
        let (r, g, b) = if has_texture {
            // Compute average color from texture pixels
            let tex = texture_opt.unwrap();
            compute_average_color(&tex.pixels)
        } else {
            // Try to infer color from material name, or use default gray
            infer_color_from_name(material_name)
        };

        // Standard material properties
        writeln!(writer, "Ns 225")?; // Specular exponent
        writeln!(writer, "Ka 1 1 1")?; // Ambient color (white)
        writeln!(writer, "Kd {:.3} {:.3} {:.3}", r, g, b)?; // Diffuse color from texture/name
        writeln!(writer, "Ks 0.5 0.5 0.5")?; // Specular color (gray)
        writeln!(writer, "Ke 0 0 0")?; // Emissive color (none)
        writeln!(writer, "Ni 1.45")?; // Optical density
        writeln!(writer, "d 1")?; // Dissolve (fully opaque)
        writeln!(writer, "illum 2")?; // Illumination model (diffuse + specular)

        // Diffuse texture map (only if we have the texture)
        if has_texture {
            let safe_name = material_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
            writeln!(writer, "map_Kd textures/{}.png", safe_name)?;
        }

        writeln!(writer)?;
    }

    // Flush and close
    writer.flush()?;

    Ok(())
}

/// Compute average RGB color from texture pixel data.
/// Returns (r, g, b) as floats in range 0.0-1.0.
fn compute_average_color(pixels: &[u8]) -> (f32, f32, f32) {
    if pixels.is_empty() || pixels.len() % 3 != 0 {
        return (0.5, 0.5, 0.5); // Default gray
    }

    let pixel_count = pixels.len() / 3;
    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;

    for chunk in pixels.chunks(3) {
        r_sum += chunk[0] as u64;
        g_sum += chunk[1] as u64;
        b_sum += chunk[2] as u64;
    }

    let r = (r_sum as f32 / pixel_count as f32) / 255.0;
    let g = (g_sum as f32 / pixel_count as f32) / 255.0;
    let b = (b_sum as f32 / pixel_count as f32) / 255.0;

    (r, g, b)
}

/// Infer a color from material name for common texture names.
/// Returns (r, g, b) as floats in range 0.0-1.0.
fn infer_color_from_name(name: &str) -> (f32, f32, f32) {
    let lower = name.to_lowercase();

    // Common color names
    if lower.contains("black") {
        return (0.1, 0.1, 0.1);
    }
    if lower.contains("white") {
        return (0.95, 0.95, 0.95);
    }
    if lower.contains("red") {
        return (0.8, 0.2, 0.2);
    }
    if lower.contains("green") || lower.contains("grn") {
        return (0.2, 0.7, 0.2);
    }
    if lower.contains("blue") || lower.contains("blu") {
        return (0.2, 0.3, 0.8);
    }
    if lower.contains("yellow") || lower.contains("yel") {
        return (0.9, 0.85, 0.2);
    }
    if lower.contains("orange") {
        return (0.9, 0.5, 0.1);
    }
    if lower.contains("brown") || lower.contains("wood") {
        return (0.55, 0.35, 0.2);
    }
    if lower.contains("gray") || lower.contains("grey") {
        return (0.5, 0.5, 0.5);
    }
    if lower.contains("metal") || lower.contains("steel") {
        return (0.6, 0.6, 0.65);
    }
    if lower.contains("concrete") || lower.contains("cement") {
        return (0.6, 0.58, 0.55);
    }
    if lower.contains("brick") {
        return (0.65, 0.35, 0.25);
    }
    if lower.contains("glass") {
        return (0.7, 0.85, 0.9);
    }
    if lower.contains("tire") || lower.contains("rubber") {
        return (0.15, 0.15, 0.15);
    }
    if lower.contains("grass") || lower.contains("grnd") || lower.contains("ground") {
        return (0.35, 0.45, 0.25);
    }
    if lower.contains("sand") || lower.contains("dirt") {
        return (0.7, 0.6, 0.4);
    }
    if lower.contains("sky") {
        return (0.5, 0.7, 0.9);
    }
    if lower.contains("light") {
        return (1.0, 0.95, 0.8);
    }
    if lower.contains("sign") {
        return (0.8, 0.75, 0.6);
    }

    // Default: medium gray
    (0.5, 0.5, 0.5)
}

/// Write a texture to a PNG file.
fn write_texture_png(texture: &crate::bsp::TextureData, path: &Path) -> std::io::Result<()> {
    use image::{ImageBuffer, Rgb};
    use std::io::Error;

    // Validate pixel data size
    let expected_size = (texture.width * texture.height * 3) as usize;
    if texture.pixels.len() != expected_size {
        return Err(Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Texture '{}' has {} bytes but expected {} ({}x{}x3)",
                texture.name,
                texture.pixels.len(),
                expected_size,
                texture.width,
                texture.height
            ),
        ));
    }

    // Create image buffer from RGB pixels
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(texture.width, texture.height, texture.pixels.clone())
            .ok_or_else(|| {
                Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to create image buffer for '{}'", texture.name),
                )
            })?;

    // Save as PNG
    img.save(path).map_err(|e| {
        Error::new(std::io::ErrorKind::Other, format!("PNG save error: {}", e))
    })?;

    Ok(())
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp::BspData;

    #[test]
    fn test_write_simple_mtl() {
        // Create minimal test data
        let data = BspData {
            map_name: "test".to_string(),
            vertices: vec![],
            faces: vec![],
            materials: vec![
                "textures/common/brick".to_string(),
                "textures/common/metal".to_string(),
            ],
            textures: vec![],
        };

        // Write to temp directory
        let temp_dir = std::env::temp_dir().join("bsp_converter_mtl_test");
        let result = write_mtl(&data, &temp_dir);

        assert!(result.is_ok());

        // Verify file exists
        let mtl_path = temp_dir.join("test.mtl");
        assert!(mtl_path.exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
