//! # BSP Module
//!
//! This module contains BSP file parsing logic for various game formats.
//!
//! ## Submodules
//!
//! - `header` — BSP header parsing (signature, version detection)
//! - `lump` — Lump table definitions and reading
//! - `cod1` — Call of Duty 1 BSP format
//! - `cod2` — Call of Duty 2 BSP format (placeholder)
//! - `mohaa` — Medal of Honor: Allied Assault BSP format (placeholder)

pub mod header;
pub mod lump;
pub mod cod1;
pub mod cod2;
pub mod mohaa;
pub mod goldsrc;

// -----------------------------------------------------------------------------
// Common types shared across BSP formats
// -----------------------------------------------------------------------------

/// Parsed BSP data ready for OBJ export.
///
/// This struct holds the extracted geometry and material information
/// in a format-agnostic way, so the OBJ writer doesn't need to know
/// which BSP format the data came from.
#[derive(Debug, Clone)]
pub struct BspData {
    /// Name of the map (derived from filename).
    pub map_name: String,

    /// All vertices in the map.
    pub vertices: Vec<Vertex>,

    /// All faces (triangle groups) in the map.
    pub faces: Vec<Face>,

    /// Material names referenced by faces.
    pub materials: Vec<String>,

    /// Extracted texture data (optional, may be empty for external textures).
    pub textures: Vec<TextureData>,
}

/// Extracted texture data from BSP.
#[derive(Debug, Clone)]
pub struct TextureData {
    /// Texture name (used as filename when exporting).
    pub name: String,

    /// Width in pixels.
    pub width: u32,

    /// Height in pixels.
    pub height: u32,

    /// RGB pixel data (width * height * 3 bytes).
    /// Empty if texture is external (e.g., in WAD file).
    pub pixels: Vec<u8>,
}

/// A single vertex with position, UV, and normal.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    /// 3D position (x, y, z).
    pub position: [f32; 3],

    /// Texture coordinates (u, v).
    pub uv: [f32; 2],

    /// Normal vector (nx, ny, nz).
    pub normal: [f32; 3],
}

/// A face (triangle group) referencing vertices and a material.
#[derive(Debug, Clone)]
pub struct Face {
    /// Index into `BspData::materials` for this face's material.
    pub material_index: usize,

    /// Indices into `BspData::vertices` for each triangle.
    /// Length should be a multiple of 3 (each group of 3 = one triangle).
    pub indices: Vec<usize>,
}
