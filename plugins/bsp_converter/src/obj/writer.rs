//! # OBJ File Writer
//!
//! This module writes Wavefront OBJ files from BSP data.
//!
//! ## OBJ Format Overview
//!
//! OBJ is a simple text-based 3D model format:
//! - `v x y z` — Vertex position
//! - `vt u v` — Texture coordinate
//! - `vn nx ny nz` — Vertex normal
//! - `f v1/vt1/vn1 v2/vt2/vn2 v3/vt3/vn3` — Face (triangle)
//! - `usemtl name` — Use material
//! - `mtllib file.mtl` — Reference material library
//!
//! ## Important notes
//!
//! - OBJ indices are **1-based**, not 0-based!
//! - V texture coordinate is often flipped (1 - v) for some engines.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::bsp::BspData;
use crate::BspResult;

// -----------------------------------------------------------------------------
// OBJ Writer
// -----------------------------------------------------------------------------

/// Write BSP data to an OBJ file.
///
/// Creates the output directory if it doesn't exist, then writes:
/// - `<map_name>.obj` — The geometry file
///
/// # Arguments
///
/// * `data` - The parsed BSP data to export.
/// * `output_dir` - Directory where the OBJ file will be written.
///
/// # Returns
///
/// `Ok(())` on success, or an IO error.
///
/// # Example
///
/// ```ignore
/// let data = cod1::parse("maps/mp_harbor.bsp")?;
/// write_obj(&data, "output/")?;
/// // Creates: output/mp_harbor.obj
/// ```
pub fn write_obj<P: AsRef<Path>>(data: &BspData, output_dir: P) -> BspResult<()> {
    let output_dir = output_dir.as_ref();

    // Create output directory if needed
    fs::create_dir_all(output_dir)?;

    // Build output path
    let obj_path = output_dir.join(format!("{}.obj", data.map_name));

    // Open file for writing
    let file = File::create(&obj_path)?;
    let mut writer = BufWriter::new(file);

    // Write header comment
    writeln!(writer, "# OBJ file exported by bsp_converter")?;
    writeln!(writer, "# Map: {}", data.map_name)?;
    writeln!(writer, "# Vertices: {}", data.vertices.len())?;
    writeln!(writer, "# Faces: {}", data.faces.len())?;
    writeln!(writer)?;

    // Reference the material library
    writeln!(writer, "mtllib {}.mtl", data.map_name)?;
    writeln!(writer)?;

    // Write all vertices
    // Format: v x y z
    for vertex in &data.vertices {
        writeln!(
            writer,
            "v {} {} {}",
            vertex.position[0], vertex.position[1], vertex.position[2]
        )?;
    }
    writeln!(writer)?;

    // Write all texture coordinates
    // Format: vt u v
    // Note: V is flipped (negated) to match common conventions
    for vertex in &data.vertices {
        writeln!(writer, "vt {} {}", vertex.uv[0], -vertex.uv[1])?;
    }
    writeln!(writer)?;

    // Write all normals
    // Format: vn nx ny nz
    for vertex in &data.vertices {
        writeln!(
            writer,
            "vn {} {} {}",
            vertex.normal[0], vertex.normal[1], vertex.normal[2]
        )?;
    }
    writeln!(writer)?;

    // Write faces grouped by material
    let mut object_index = 0;

    for face in &data.faces {
        // Get material name (or use index if out of bounds)
        let material_name = data
            .materials
            .get(face.material_index)
            .cloned()
            .unwrap_or_else(|| format!("material_{}", face.material_index));

        // Write object/group header
        writeln!(writer, "g {}_{}", data.map_name, object_index)?;
        writeln!(writer, "usemtl {}", material_name)?;

        // Write triangles
        // OBJ indices are 1-based, so add 1 to each index
        for chunk in face.indices.chunks(3) {
            if chunk.len() == 3 {
                // Format: f v1/vt1/vn1 v2/vt2/vn2 v3/vt3/vn3
                // Since we write v, vt, vn in the same order, indices are the same
                let i1 = chunk[0] + 1;
                let i2 = chunk[1] + 1;
                let i3 = chunk[2] + 1;

                writeln!(
                    writer,
                    "f {}/{}/{} {}/{}/{} {}/{}/{}",
                    i1, i1, i1, i2, i2, i2, i3, i3, i3
                )?;
            }
        }

        writeln!(writer)?;
        object_index += 1;
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
    use crate::bsp::{Face, Vertex};

    #[test]
    fn test_write_simple_obj() {
        // Create minimal test data
        let data = BspData {
            map_name: "test".to_string(),
            vertices: vec![
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    uv: [0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    uv: [1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    uv: [0.0, 1.0],
                    normal: [0.0, 0.0, 1.0],
                },
            ],
            faces: vec![Face {
                material_index: 0,
                indices: vec![0, 1, 2],
            }],
            materials: vec!["test_material".to_string()],
            textures: vec![],
        };

        // Write to temp directory
        let temp_dir = std::env::temp_dir().join("bsp_converter_test");
        let result = write_obj(&data, &temp_dir);

        assert!(result.is_ok());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
