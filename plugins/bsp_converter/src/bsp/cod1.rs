//! # Call of Duty 1 BSP Format
//!
//! This module implements parsing for CoD1 BSP files (version 59).
//!
//! ## Data layout
//!
//! CoD1 BSP files contain:
//! - Header (8 bytes): signature + version
//! - Lump table (264 bytes): 33 lump entries
//! - Lump data: materials, vertices, faces, mesh indices, etc.
//!
//! ## Key lumps
//!
//! - Lump 0: Materials (64-byte names + flags)
//! - Lump 6: Triangle soups (face groups)
//! - Lump 7: Vertices (position, UV, normal)
//! - Lump 8: Mesh vertex indices

use bytemuck::{Pod, Zeroable};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::binary::{bytes_to_string, seek_to};
use crate::bsp::lump::{lump_index, read_lump_data, read_lump_table};
use crate::bsp::{BspData, Face, Vertex};
use crate::BspResult;

// -----------------------------------------------------------------------------
// CoD1-specific structs
// -----------------------------------------------------------------------------

/// Material entry in CoD1 BSP.
///
/// Each material has a 64-character name and some flags.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Cod1Material {
    /// Material name (null-terminated, 64 bytes).
    pub name: [u8; 64],

    /// Material flags.
    pub flags: u32,

    /// Content flags.
    pub content_flags: u32,
}

impl Cod1Material {
    /// Get the material name as a String.
    pub fn name_string(&self) -> String {
        bytes_to_string(&self.name)
    }
}

/// Vertex in CoD1 BSP.
///
/// Contains position, UV coordinates, and normal vector.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Cod1Vertex {
    /// 3D position (x, y, z).
    pub position: [f32; 3],

    /// Texture coordinates (u, v).
    pub uv: [f32; 2],

    /// Unknown data (possibly lightmap UVs).
    pub unknown1: [f32; 2],

    /// Normal vector (nx, ny, nz).
    pub normal: [f32; 3],

    /// Unknown data.
    pub unknown2: f32,
}

/// Triangle soup (face group) in CoD1 BSP.
///
/// Groups triangles that share the same material.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Cod1TriangleSoup {
    /// Index into the materials array.
    pub material_id: u16,

    /// Draw order (for sorting).
    pub draw_order: u16,

    /// Offset into the vertex array.
    pub vertex_offset: i32,

    /// Number of vertices used by this soup.
    pub vertex_length: u16,

    /// Number of triangle indices (should be multiple of 3).
    pub triangle_length: u16,

    /// Offset into the mesh verts array.
    pub triangle_offset: i32,
}

/// Mesh vertex index in CoD1 BSP.
///
/// References a vertex in the vertex array.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Cod1MeshVert {
    /// Offset into the vertex array (relative to soup's vertex_offset).
    pub offset: u16,
}

// -----------------------------------------------------------------------------
// Parsing
// -----------------------------------------------------------------------------

/// Parse a CoD1 BSP file and return extracted geometry data.
///
/// # Arguments
///
/// * `path` - Path to the BSP file.
///
/// # Returns
///
/// A `BspData` struct containing all geometry and materials.
///
/// # Example
///
/// ```ignore
/// let data = cod1::parse("maps/mp_harbor.bsp")?;
/// println!("Map: {}", data.map_name);
/// println!("Vertices: {}", data.vertices.len());
/// println!("Faces: {}", data.faces.len());
/// ```
pub fn parse<P: AsRef<Path>>(path: P) -> BspResult<BspData> {
    let path = path.as_ref();

    // Extract map name from filename
    let map_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Open file
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Skip header (already validated by caller)
    seek_to(&mut reader, 8)?;

    // Read lump table
    let lump_table = read_lump_table(&mut reader)?;

    // Read materials
    let raw_materials: Vec<Cod1Material> =
        read_lump_data(&mut reader, &lump_table.lumps[lump_index::MATERIALS])?;

    // Read triangle soups (faces)
    let raw_soups: Vec<Cod1TriangleSoup> =
        read_lump_data(&mut reader, &lump_table.lumps[lump_index::TRIANGLE_SOUPS])?;

    // Read vertices
    let raw_vertices: Vec<Cod1Vertex> =
        read_lump_data(&mut reader, &lump_table.lumps[lump_index::VERTICES])?;

    // Read mesh vertex indices
    let raw_mesh_verts: Vec<Cod1MeshVert> =
        read_lump_data(&mut reader, &lump_table.lumps[lump_index::MESH_VERTS])?;

    // Convert to format-agnostic types
    let materials: Vec<String> = raw_materials
        .iter()
        .map(|m| m.name_string().replace('\\', "/").to_lowercase())
        .collect();

    let vertices: Vec<Vertex> = raw_vertices
        .iter()
        .map(|v| Vertex {
            position: v.position,
            uv: v.uv,
            normal: v.normal,
        })
        .collect();

    // Build faces from triangle soups
    let mut faces = Vec::with_capacity(raw_soups.len());

    for soup in &raw_soups {
        let mut indices = Vec::new();

        // Each triangle is 3 consecutive mesh vert indices
        let start = soup.triangle_offset as usize;
        let end = start + soup.triangle_length as usize;

        for i in (start..end).step_by(3) {
            if i + 2 < raw_mesh_verts.len() {
                // Calculate actual vertex indices
                let base = soup.vertex_offset as usize;
                let i1 = base + raw_mesh_verts[i].offset as usize;
                let i2 = base + raw_mesh_verts[i + 1].offset as usize;
                let i3 = base + raw_mesh_verts[i + 2].offset as usize;

                indices.push(i1);
                indices.push(i2);
                indices.push(i3);
            }
        }

        faces.push(Face {
            material_index: soup.material_id as usize,
            indices,
        });
    }

    Ok(BspData {
        map_name,
        vertices,
        faces,
        materials,
    })
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_sizes() {
        // Verify struct sizes match C# equivalents
        assert_eq!(std::mem::size_of::<Cod1Material>(), 72); // 64 + 4 + 4
        assert_eq!(std::mem::size_of::<Cod1Vertex>(), 44); // 12 + 8 + 8 + 12 + 4
        assert_eq!(std::mem::size_of::<Cod1TriangleSoup>(), 16); // 2 + 2 + 4 + 2 + 2 + 4
        assert_eq!(std::mem::size_of::<Cod1MeshVert>(), 2);
    }

    #[test]
    fn test_material_name() {
        let mut mat = Cod1Material {
            name: [0u8; 64],
            flags: 0,
            content_flags: 0,
        };

        // Set name
        let name = b"textures/common/clip";
        mat.name[..name.len()].copy_from_slice(name);

        assert_eq!(mat.name_string(), "textures/common/clip");
    }
}
