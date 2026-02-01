//! # BSP Lump Definitions
//!
//! This module defines the lump table structures used in BSP files.
//!
//! ## What are lumps?
//!
//! BSP files store data in "lumps" — contiguous chunks of data at specific
//! offsets in the file. The lump table (stored after the header) tells us
//! where each lump is located and how large it is.
//!
//! ## Lump types (CoD1/CoD2)
//!
//! Different lumps contain different data:
//! - Lump 0: Materials/textures
//! - Lump 6: Triangle soups (face groups)
//! - Lump 7: Vertices
//! - Lump 8: Mesh vertex indices
//!
//! The exact lump indices may vary by game version.

use bytemuck::{Pod, Zeroable};
use std::io::{Read, Seek};

use crate::binary::{read_struct, read_struct_array, seek_to};
use crate::BspResult;

// -----------------------------------------------------------------------------
// Lump entry struct
// -----------------------------------------------------------------------------

/// A single lump entry in the lump table.
///
/// Each entry describes where a lump's data is located in the file.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct LumpEntry {
    /// Size of the lump data in bytes.
    pub length: i32,

    /// Offset from the start of the file to the lump data.
    pub offset: i32,
}

impl LumpEntry {
    /// Check if this lump contains any data.
    pub fn is_empty(&self) -> bool {
        self.length <= 0
    }

    /// Calculate how many items of type T fit in this lump.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The struct type stored in this lump.
    pub fn count<T>(&self) -> usize {
        if self.length <= 0 {
            0
        } else {
            self.length as usize / std::mem::size_of::<T>()
        }
    }
}

// -----------------------------------------------------------------------------
// Lump table
// -----------------------------------------------------------------------------

/// Number of lumps in CoD1/CoD2 BSP files.
pub const COD_LUMP_COUNT: usize = 33;

/// The complete lump table for CoD-style BSP files.
///
/// Note: We don't derive Pod/Zeroable because bytemuck doesn't support
/// arrays larger than 32 elements. Use `read_lump_table` instead.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CodLumpTable {
    /// Array of lump entries.
    pub lumps: Vec<LumpEntry>,
}

// -----------------------------------------------------------------------------
// Lump indices (CoD1/CoD2)
// -----------------------------------------------------------------------------

/// Lump index constants for CoD1/CoD2 BSP files.
///
/// These define which lump index contains which type of data.
pub mod lump_index {
    /// Materials/textures lump.
    pub const MATERIALS: usize = 0;

    /// Triangle soups (face groups) lump.
    pub const TRIANGLE_SOUPS: usize = 6;

    /// Vertices lump.
    pub const VERTICES: usize = 7;

    /// Mesh vertex indices lump.
    pub const MESH_VERTS: usize = 8;
}

// -----------------------------------------------------------------------------
// Lump reading utilities
// -----------------------------------------------------------------------------

/// Read the lump table from a BSP file.
///
/// The lump table immediately follows the 8-byte header.
///
/// # Arguments
///
/// * `reader` - A reader positioned after the header (at offset 8).
pub fn read_lump_table<R: Read>(reader: &mut R) -> BspResult<CodLumpTable> {
    let lumps: Vec<LumpEntry> = read_struct_array(reader, COD_LUMP_COUNT)?;
    Ok(CodLumpTable { lumps })
}

/// Read all items of type T from a specific lump.
///
/// # Type Parameters
///
/// * `T` - The struct type to read. Must be `Pod + Zeroable + Copy`.
///
/// # Arguments
///
/// * `reader` - A seekable reader for the BSP file.
/// * `lump` - The lump entry describing where to read from.
///
/// # Returns
///
/// A `Vec<T>` containing all items in the lump.
pub fn read_lump_data<T, R>(reader: &mut R, lump: &LumpEntry) -> BspResult<Vec<T>>
where
    T: Pod + Zeroable + Copy,
    R: Read + Seek,
{
    // Seek to lump offset
    seek_to(reader, lump.offset as u64)?;

    // Calculate item count
    let count = lump.count::<T>();

    // Read all items
    read_struct_array(reader, count)
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lump_entry_size() {
        // Each lump entry is 8 bytes (4 length + 4 offset)
        assert_eq!(std::mem::size_of::<LumpEntry>(), 8);
    }

    #[test]
    fn test_lump_entry_count() {
        // CoD BSP files have 33 lumps
        assert_eq!(COD_LUMP_COUNT, 33);
    }

    #[test]
    fn test_lump_count() {
        let lump = LumpEntry {
            length: 100,
            offset: 0,
        };

        // If each item is 10 bytes, we should have 10 items
        #[repr(C, packed)]
        #[derive(Copy, Clone, Pod, Zeroable)]
        struct TenBytes([u8; 10]);

        assert_eq!(lump.count::<TenBytes>(), 10);
    }
}
