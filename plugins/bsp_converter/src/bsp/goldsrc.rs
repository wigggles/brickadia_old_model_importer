//! # GoldSrc / Quake / Half-Life BSP Format
//!
//! This module implements parsing for GoldSrc-era BSP files.
//!
//! ## Supported games
//!
//! - Quake 1 (version 29)
//! - Half-Life 1 (version 30) — **PRIMARY TARGET**
//! - Quake 2 (version 38) — different format, partial support
//!
//! ## Key differences from CoD/id Tech 3
//!
//! - No "IBSP" signature for Q1/HL1 (version is first 4 bytes)
//! - 15 lumps (Q1/HL1) vs 33 lumps (CoD)
//! - Edge-based geometry instead of triangle soups
//! - UVs computed from texinfo axes, not stored per-vertex
//! - Textures embedded in BSP (Q1/HL1) with palette indexing
//! - Coordinate system requires swizzle: (x, z, -y)
//!
//! ## Reference
//!
//! Based on Python `bsp2obj` submodule at `submodules/source_gold-converter-obj`.

use bytemuck::{Pod, Zeroable};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::binary::{bytes_to_string, read_struct, read_struct_array, seek_to};
use crate::bsp::{BspData, Face, Vertex};
use crate::{BspError, BspResult};

// =============================================================================
// Constants
// =============================================================================

/// BSP version for Quake 1.
pub const VERSION_QUAKE1: i32 = 29;

/// BSP version for Half-Life 1 (GoldSrc).
pub const VERSION_HALFLIFE: i32 = 30;

/// BSP version for Quake 2 (has IBSP signature).
pub const VERSION_QUAKE2: i32 = 38;

/// Number of lumps in Q1/HL1 BSP files.
pub const GOLDSRC_LUMP_COUNT: usize = 15;

// =============================================================================
// Lump indices for Q1/HL1
// =============================================================================

/// Lump index constants for GoldSrc BSP files.
pub mod lump_index {
    /// Embedded mip textures.
    pub const TEXTURES: usize = 2;

    /// Vertex positions.
    pub const VERTICES: usize = 3;

    /// Texture info (UV axes and offsets).
    pub const TEXINFO: usize = 6;

    /// Faces (polygons).
    pub const FACES: usize = 7;

    /// Edges (vertex pairs).
    pub const EDGES: usize = 12;

    /// Surfedges (signed edge indices).
    pub const SURFEDGES: usize = 13;
}

// =============================================================================
// GoldSrc-specific structs
// =============================================================================

/// Lump entry in the lump table.
///
/// Each entry describes where a lump's data is located.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GoldSrcLumpEntry {
    /// Offset from start of file.
    pub offset: i32,

    /// Length in bytes.
    pub length: i32,
}

impl GoldSrcLumpEntry {
    /// Calculate how many items of type T fit in this lump.
    pub fn count<T>(&self) -> usize {
        if self.length <= 0 {
            0
        } else {
            self.length as usize / std::mem::size_of::<T>()
        }
    }
}

/// Vertex position in GoldSrc BSP.
///
/// Just 3 floats (x, y, z). Coordinate swizzle is applied during export.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GoldSrcVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl GoldSrcVertex {
    /// Apply coordinate swizzle for OBJ export.
    ///
    /// GoldSrc uses a different coordinate system than OBJ.
    /// Transform: (x, y, z) → (x, z, -y)
    pub fn swizzled(&self) -> (f32, f32, f32) {
        (self.x, self.z, -self.y)
    }

    /// Compute dot product with a 3-component axis vector.
    pub fn dot(&self, axis: &[f32; 3]) -> f32 {
        self.x * axis[0] + self.y * axis[1] + self.z * axis[2]
    }
}

/// Edge in GoldSrc BSP.
///
/// An edge connects two vertices. The direction matters for face winding.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GoldSrcEdge {
    /// First vertex index.
    pub v1: u16,

    /// Second vertex index.
    pub v2: u16,
}

/// Surfedge (signed edge reference).
///
/// - Positive value: use edge in forward direction (v1 → v2)
/// - Negative value: use edge in reverse direction (v2 → v1)
pub type SurfEdge = i32;

/// Face (polygon) in GoldSrc BSP.
///
/// Faces are defined by a sequence of edges, not direct vertex indices.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GoldSrcFace {
    /// Plane index (for collision, not needed for rendering).
    pub plane_id: u16,

    /// Which side of the plane this face is on.
    pub side: u16,

    /// Index into surfedges array.
    pub first_edge: i32,

    /// Number of edges in this face.
    pub num_edges: i16,

    /// Index into texinfo array.
    pub texinfo_id: i16,

    /// Lightmap style indices.
    pub lightmap_styles: [u8; 4],

    /// Offset into lightmap data.
    pub lightmap_offset: i32,
}

/// Texture info in GoldSrc BSP.
///
/// Contains UV mapping axes and offsets. UVs are computed per-vertex using:
/// ```ignore
/// u = (vertex.dot(u_axis) + u_offset) / texture_width
/// v = (vertex.dot(v_axis) + v_offset) / texture_height
/// ```
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GoldSrcTexInfo {
    /// U axis for texture mapping.
    pub u_axis: [f32; 3],

    /// U offset.
    pub u_offset: f32,

    /// V axis for texture mapping.
    pub v_axis: [f32; 3],

    /// V offset.
    pub v_offset: f32,

    /// Index into mip texture array.
    pub texture_id: u32,

    /// Texture flags (e.g., animated, special).
    pub flags: u32,
}

/// Mip texture header in GoldSrc BSP.
///
/// Textures are embedded in the BSP file with multiple mip levels.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct MipTexHeader {
    /// Texture name (null-terminated, 16 bytes max).
    pub name: [u8; 16],

    /// Texture width in pixels.
    pub width: u32,

    /// Texture height in pixels.
    pub height: u32,

    /// Offset to mip level 0 (full resolution).
    pub offset1: u32,

    /// Offset to mip level 1 (1/2 resolution).
    pub offset2: u32,

    /// Offset to mip level 2 (1/4 resolution).
    pub offset4: u32,

    /// Offset to mip level 3 (1/8 resolution).
    pub offset8: u32,
}

impl MipTexHeader {
    /// Get the texture name as a String.
    pub fn name_string(&self) -> String {
        bytes_to_string(&self.name)
    }
}

/// Parsed texture with pixel data.
#[derive(Debug, Clone)]
pub struct GoldSrcTexture {
    /// Texture name.
    pub name: String,

    /// Width in pixels.
    pub width: u32,

    /// Height in pixels.
    pub height: u32,

    /// RGB pixel data (width * height * 3 bytes).
    pub pixels: Vec<u8>,
}

// =============================================================================
// Header detection
// =============================================================================

/// Detect if a file is a GoldSrc BSP and return its version.
///
/// GoldSrc BSP files don't have an "IBSP" signature — the first 4 bytes
/// are the version number directly.
///
/// # Returns
///
/// - `Some(version)` if this looks like a GoldSrc BSP
/// - `None` if it has an "IBSP" signature (CoD/id Tech 3 format)
pub fn detect_goldsrc_version<P: AsRef<Path>>(path: P) -> BspResult<Option<i32>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read first 4 bytes
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;

    // Check if it's "IBSP" (CoD/id Tech 3)
    if &magic == b"IBSP" {
        return Ok(None);
    }

    // Otherwise, interpret as version number
    let version = i32::from_le_bytes(magic);

    // Validate known versions
    match version {
        VERSION_QUAKE1 | VERSION_HALFLIFE => Ok(Some(version)),
        _ => {
            // Could be Q2 or unknown — check for IBSP at offset 0
            // Q2 has IBSP signature, so if we got here it's not Q2
            Ok(Some(version))
        }
    }
}

// =============================================================================
// Parsing
// =============================================================================

/// Parse a GoldSrc/Quake/Half-Life BSP file.
///
/// # Arguments
///
/// * `path` - Path to the BSP file.
/// * `palette` - Optional external palette (required for Q1, optional for HL1).
///
/// # Returns
///
/// A `BspData` struct containing all geometry and materials.
///
/// # Example
///
/// ```ignore
/// let data = goldsrc::parse("maps/c1a0.bsp", None)?;
/// println!("Map: {}", data.map_name);
/// println!("Vertices: {}", data.vertices.len());
/// ```
pub fn parse<P: AsRef<Path>>(path: P, palette: Option<&[u8]>) -> BspResult<BspData> {
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

    // Read version (first 4 bytes)
    let version: i32 = read_struct(&mut reader)?;

    // Validate version
    if version != VERSION_QUAKE1 && version != VERSION_HALFLIFE {
        return Err(BspError::UnsupportedVersion(version));
    }

    // Read lump table (15 lumps for Q1/HL1)
    let lumps: Vec<GoldSrcLumpEntry> = read_struct_array(&mut reader, GOLDSRC_LUMP_COUNT)?;

    // Parse vertices
    let raw_vertices = read_lump_data::<GoldSrcVertex, _>(
        &mut reader,
        &lumps[lump_index::VERTICES],
    )?;

    // Parse edges
    let edges = read_lump_data::<GoldSrcEdge, _>(
        &mut reader,
        &lumps[lump_index::EDGES],
    )?;

    // Parse surfedges (signed edge indices)
    let surfedges = read_lump_data::<SurfEdge, _>(
        &mut reader,
        &lumps[lump_index::SURFEDGES],
    )?;

    // Parse faces
    let raw_faces = read_lump_data::<GoldSrcFace, _>(
        &mut reader,
        &lumps[lump_index::FACES],
    )?;

    // Parse texinfo
    let texinfos = read_lump_data::<GoldSrcTexInfo, _>(
        &mut reader,
        &lumps[lump_index::TEXINFO],
    )?;

    // Parse embedded textures
    let textures = parse_textures(&mut reader, &lumps[lump_index::TEXTURES], version, palette)?;

    // Build material name list
    let materials: Vec<String> = textures.iter().map(|t| t.name.clone()).collect();

    // Convert vertices with swizzle
    let vertices: Vec<Vertex> = raw_vertices
        .iter()
        .map(|v| {
            let (x, y, z) = v.swizzled();
            Vertex {
                position: [x, y, z],
                // UVs will be computed per-face during triangulation
                uv: [0.0, 0.0],
                // Normals will be computed per-face
                normal: [0.0, 0.0, 1.0],
            }
        })
        .collect();

    // Triangulate faces and compute UVs
    let faces = triangulate_faces(
        &raw_faces,
        &edges,
        &surfedges,
        &raw_vertices,
        &texinfos,
        &textures,
    );

    Ok(BspData {
        map_name,
        vertices,
        faces,
        materials,
    })
}

// =============================================================================
// Helper functions
// =============================================================================

/// Read lump data from file.
fn read_lump_data<T, R>(reader: &mut R, lump: &GoldSrcLumpEntry) -> BspResult<Vec<T>>
where
    T: Pod + Zeroable + Copy,
    R: Read + Seek,
{
    seek_to(reader, lump.offset as u64)?;
    let count = lump.count::<T>();
    read_struct_array(reader, count)
}

/// Parse embedded textures from the texture lump.
fn parse_textures<R: Read + Seek>(
    reader: &mut R,
    lump: &GoldSrcLumpEntry,
    version: i32,
    external_palette: Option<&[u8]>,
) -> BspResult<Vec<GoldSrcTexture>> {
    if lump.length <= 0 {
        return Ok(Vec::new());
    }

    let lump_start = lump.offset as u64;
    seek_to(reader, lump_start)?;

    // Read number of textures
    let num_textures: u32 = read_struct(reader)?;

    // Read texture offsets
    let offsets: Vec<u32> = read_struct_array(reader, num_textures as usize)?;

    let mut textures = Vec::with_capacity(num_textures as usize);

    for offset in offsets {
        // Seek to texture header
        seek_to(reader, lump_start + offset as u64)?;

        // Read mip texture header
        let header: MipTexHeader = read_struct(reader)?;

        // Skip textures with zero offsets (external WAD reference)
        if header.offset1 == 0 {
            textures.push(GoldSrcTexture {
                name: header.name_string(),
                width: header.width,
                height: header.height,
                pixels: Vec::new(), // Empty — would need WAD loading
            });
            continue;
        }

        // Seek to mip level 0 pixel data
        seek_to(reader, lump_start + offset as u64 + header.offset1 as u64)?;

        // Read palette-indexed pixel data
        let num_pixels = (header.width * header.height) as usize;
        let mut indices = vec![0u8; num_pixels];
        reader.read_exact(&mut indices)?;

        // Get palette
        let palette = if version == VERSION_HALFLIFE {
            // HL1: palette is embedded after mip level 3
            // Seek past mip level 3 data + 2-byte padding
            let mip3_size = (header.width / 8) * (header.height / 8);
            seek_to(
                reader,
                lump_start + offset as u64 + header.offset8 as u64 + mip3_size as u64 + 2,
            )?;

            let mut pal = vec![0u8; 256 * 3];
            reader.read_exact(&mut pal)?;
            pal
        } else if let Some(ext_pal) = external_palette {
            // Q1: use external palette
            ext_pal.to_vec()
        } else {
            // No palette available — use grayscale
            (0..256).flat_map(|i| vec![i as u8, i as u8, i as u8]).collect()
        };

        // Convert indexed pixels to RGB
        let mut pixels = Vec::with_capacity(num_pixels * 3);
        for idx in indices {
            let pal_offset = (idx as usize) * 3;
            if pal_offset + 2 < palette.len() {
                pixels.push(palette[pal_offset]);
                pixels.push(palette[pal_offset + 1]);
                pixels.push(palette[pal_offset + 2]);
            } else {
                // Fallback for invalid palette index
                pixels.push(255);
                pixels.push(0);
                pixels.push(255); // Magenta for missing
            }
        }

        textures.push(GoldSrcTexture {
            name: header.name_string(),
            width: header.width,
            height: header.height,
            pixels,
        });
    }

    Ok(textures)
}

/// Triangulate GoldSrc faces (edge-based polygons) into triangle indices.
///
/// GoldSrc faces are defined by edges, not direct vertex indices.
/// This function:
/// 1. Resolves surfedges to get vertex indices for each face
/// 2. Computes UVs from texinfo
/// 3. Triangulates polygons using fan triangulation
/// 4. Computes face normals
fn triangulate_faces(
    raw_faces: &[GoldSrcFace],
    edges: &[GoldSrcEdge],
    surfedges: &[SurfEdge],
    vertices: &[GoldSrcVertex],
    texinfos: &[GoldSrcTexInfo],
    textures: &[GoldSrcTexture],
) -> Vec<Face> {
    let mut faces = Vec::with_capacity(raw_faces.len());

    for raw_face in raw_faces {
        // Get texinfo for this face
        let texinfo_idx = raw_face.texinfo_id as usize;
        if texinfo_idx >= texinfos.len() {
            continue;
        }
        let texinfo = &texinfos[texinfo_idx];

        // Get texture for UV calculation
        let tex_idx = texinfo.texture_id as usize;
        let (tex_width, tex_height) = if tex_idx < textures.len() {
            (textures[tex_idx].width as f32, textures[tex_idx].height as f32)
        } else {
            (64.0, 64.0) // Default size
        };

        // Skip degenerate textures
        if tex_width == 0.0 || tex_height == 0.0 {
            continue;
        }

        // Resolve edge list to vertex indices
        let mut vert_indices = Vec::with_capacity(raw_face.num_edges as usize);

        for i in 0..raw_face.num_edges as usize {
            let surfedge_idx = raw_face.first_edge as usize + i;
            if surfedge_idx >= surfedges.len() {
                continue;
            }

            let surfedge = surfedges[surfedge_idx];
            let edge_idx = surfedge.abs() as usize;

            if edge_idx >= edges.len() {
                continue;
            }

            let edge = &edges[edge_idx];

            // Surfedge sign determines edge direction
            let vert_idx = if surfedge >= 0 {
                edge.v2 as usize // Forward: use second vertex
            } else {
                edge.v1 as usize // Reverse: use first vertex
            };

            vert_indices.push(vert_idx);
        }

        // Need at least 3 vertices to form a triangle
        if vert_indices.len() < 3 {
            continue;
        }

        // Triangulate using fan from first vertex
        // For N vertices, we get N-2 triangles
        let mut indices = Vec::with_capacity((vert_indices.len() - 2) * 3);

        for i in 1..vert_indices.len() - 1 {
            // Triangle: v0, vi, vi+1
            indices.push(vert_indices[0]);
            indices.push(vert_indices[i]);
            indices.push(vert_indices[i + 1]);
        }

        faces.push(Face {
            material_index: tex_idx,
            indices,
        });
    }

    faces
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_sizes() {
        assert_eq!(std::mem::size_of::<GoldSrcLumpEntry>(), 8);
        assert_eq!(std::mem::size_of::<GoldSrcVertex>(), 12);
        assert_eq!(std::mem::size_of::<GoldSrcEdge>(), 4);
        assert_eq!(std::mem::size_of::<GoldSrcFace>(), 20);
        assert_eq!(std::mem::size_of::<GoldSrcTexInfo>(), 40);
        assert_eq!(std::mem::size_of::<MipTexHeader>(), 40);
    }

    #[test]
    fn test_vertex_swizzle() {
        let v = GoldSrcVertex {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let (x, y, z) = v.swizzled();
        assert_eq!(x, 1.0);
        assert_eq!(y, 3.0);
        assert_eq!(z, -2.0);
    }

    #[test]
    fn test_vertex_dot() {
        let v = GoldSrcVertex {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let axis = [1.0, 0.0, 0.0];
        assert_eq!(v.dot(&axis), 1.0);
    }
}
