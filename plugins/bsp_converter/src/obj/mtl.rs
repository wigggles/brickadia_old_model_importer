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

        // Standard material properties
        // These are reasonable defaults for game textures
        writeln!(writer, "Ns 225")?; // Specular exponent
        writeln!(writer, "Ka 1 1 1")?; // Ambient color (white)
        writeln!(writer, "Kd 0.8 0.8 0.8")?; // Diffuse color (light gray)
        writeln!(writer, "Ks 0.5 0.5 0.5")?; // Specular color (gray)
        writeln!(writer, "Ke 0 0 0")?; // Emissive color (none)
        writeln!(writer, "Ni 1.45")?; // Optical density
        writeln!(writer, "d 1")?; // Dissolve (fully opaque)
        writeln!(writer, "illum 2")?; // Illumination model (diffuse + specular)

        // Diffuse texture map
        // The texture path is relative to the OBJ file
        // We assume textures will be extracted to the same directory structure
        //
        // Note: We don't know the file extension here (.tga, .jpg, etc.)
        // The texture extraction step should handle finding the right extension
        writeln!(writer, "map_Kd {}", material_name)?;

        writeln!(writer)?;
    }

    // Flush and close
    writer.flush()?;

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
