//! # IWI to DDS Texture Conversion
//!
//! This module converts IWI (Infinity Ward Image) textures to DDS format.
//!
//! ## What is IWI?
//!
//! IWI is a proprietary texture format used by Call of Duty games.
//! It's essentially a DXT-compressed texture with a custom header.
//!
//! ## IWI Header Structure
//!
//! - Magic: "IWi" (3 bytes)
//! - Version: 1 byte
//! - Format: 1 byte (DXT1 = 11, DXT5 = 13)
//! - Flags: 1 byte
//! - Width: 2 bytes
//! - Height: 2 bytes
//! - ... (offsets to mipmap data)
//!
//! ## DDS Output
//!
//! The output is a standard DDS file that can be read by most image tools.
//!
//! ## Status
//!
//! **PLACEHOLDER** — Basic structure defined, full implementation TODO.

use std::io::{Read, Write};
use std::path::Path;

use bytemuck::{Pod, Zeroable};

use crate::binary::{read_struct, seek_to};
use crate::{BspError, BspResult};

// -----------------------------------------------------------------------------
// IWI Header
// -----------------------------------------------------------------------------

/// IWI file header.
///
/// Based on the C# reference in IWI_Header.cs.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct IwiHeader {
    /// Magic bytes "IWi" + version.
    pub magic: [u8; 4],

    /// Texture format (11 = DXT1, 13 = DXT5).
    pub dxt: u8,

    /// Flags.
    pub flags: u8,

    /// Texture width.
    pub width: u16,

    /// Texture height.
    pub height: u16,

    /// File size.
    pub file_size: u32,

    /// Offset to mipmap level 2.
    pub mipmap2_offset: u32,

    /// Offset to mipmap level 1.
    pub mipmap1_offset: u32,

    /// Offset to main texture data.
    pub texture_offset: u32,
}

impl IwiHeader {
    /// Expected magic bytes for IWI files.
    pub const MAGIC: &'static [u8; 3] = b"IWi";

    /// DXT1 format identifier.
    pub const DXT1: u8 = 11;

    /// DXT5 format identifier.
    pub const DXT5: u8 = 13;

    /// Check if the magic bytes are valid.
    pub fn is_valid(&self) -> bool {
        &self.magic[..3] == Self::MAGIC
    }

    /// Get the DXT format string for DDS header.
    pub fn dxt_fourcc(&self) -> [u8; 4] {
        match self.dxt {
            Self::DXT1 => *b"DXT1",
            Self::DXT5 => *b"DXT5",
            _ => *b"DXT5", // Default to DXT5
        }
    }
}

// -----------------------------------------------------------------------------
// DDS Header
// -----------------------------------------------------------------------------

/// DDS file header.
///
/// This is the standard DirectDraw Surface header format.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct DdsHeader {
    /// Magic bytes "DDS ".
    pub magic: [u8; 4],

    /// Header size (always 124).
    pub size: u32,

    /// Flags.
    pub flags: u32,

    /// Texture height.
    pub height: u32,

    /// Texture width.
    pub width: u32,

    /// Pitch or linear size.
    pub pitch_or_linear_size: u32,

    /// Depth (for volume textures).
    pub depth: u32,

    /// Mipmap count.
    pub mipmap_count: u32,

    /// Reserved.
    pub reserved1: [u32; 11],

    // Pixel format structure (embedded)
    /// Pixel format size (always 32).
    pub pf_size: u32,

    /// Pixel format flags.
    pub pf_flags: u32,

    /// FourCC code (e.g., "DXT1").
    pub pf_fourcc: [u8; 4],

    /// RGB bit count.
    pub pf_rgb_bit_count: u32,

    /// Red bit mask.
    pub pf_r_bit_mask: u32,

    /// Green bit mask.
    pub pf_g_bit_mask: u32,

    /// Blue bit mask.
    pub pf_b_bit_mask: u32,

    /// Alpha bit mask.
    pub pf_a_bit_mask: u32,

    /// Caps.
    pub caps: u32,

    /// Caps2.
    pub caps2: u32,

    /// Caps3.
    pub caps3: u32,

    /// Caps4.
    pub caps4: u32,

    /// Reserved.
    pub reserved2: u32,
}

impl DdsHeader {
    /// Create a DDS header from an IWI header.
    pub fn from_iwi(iwi: &IwiHeader) -> Self {
        Self {
            magic: *b"DDS ",
            size: 124,
            flags: 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000, // CAPS | HEIGHT | WIDTH | PIXELFORMAT | LINEARSIZE
            height: iwi.height as u32,
            width: iwi.width as u32,
            pitch_or_linear_size: iwi.file_size - iwi.texture_offset,
            depth: 0,
            mipmap_count: 0,
            reserved1: [0; 11],
            pf_size: 32,
            pf_flags: 0x4, // FOURCC
            pf_fourcc: iwi.dxt_fourcc(),
            pf_rgb_bit_count: 0,
            pf_r_bit_mask: 0,
            pf_g_bit_mask: 0,
            pf_b_bit_mask: 0,
            pf_a_bit_mask: 0,
            caps: 0x1000, // TEXTURE
            caps2: 0,
            caps3: 0,
            caps4: 0,
            reserved2: 0,
        }
    }
}

// -----------------------------------------------------------------------------
// Conversion
// -----------------------------------------------------------------------------

/// Convert an IWI file to DDS format.
///
/// # Arguments
///
/// * `iwi_path` - Path to the input IWI file.
/// * `dds_path` - Path to the output DDS file.
///
/// # Returns
///
/// `Ok(())` on success, or an error.
///
/// # Example
///
/// ```ignore
/// convert_iwi_to_dds("texture.iwi", "texture.dds")?;
/// ```
pub fn convert_iwi_to_dds<P: AsRef<Path>, Q: AsRef<Path>>(
    iwi_path: P,
    dds_path: Q,
) -> BspResult<()> {
    let iwi_path = iwi_path.as_ref();
    let dds_path = dds_path.as_ref();

    // Open IWI file
    let mut iwi_file = std::fs::File::open(iwi_path)?;

    // Read IWI header
    let header: IwiHeader = read_struct(&mut iwi_file)?;

    if !header.is_valid() {
        return Err(BspError::TextureError(format!(
            "Invalid IWI file: {}",
            iwi_path.display()
        )));
    }

    // Calculate mipmap sizes
    let lowest_mipmap_size = (header.mipmap1_offset - header.mipmap2_offset) as usize;

    // Read mipmap data
    seek_to(&mut iwi_file, header.mipmap2_offset as u64)?;
    let mut mipmap2 = vec![0u8; lowest_mipmap_size];
    iwi_file.read_exact(&mut mipmap2)?;

    seek_to(&mut iwi_file, header.mipmap1_offset as u64)?;
    let mut mipmap1 = vec![0u8; lowest_mipmap_size * 4];
    iwi_file.read_exact(&mut mipmap1)?;

    seek_to(&mut iwi_file, header.texture_offset as u64)?;
    let mut texture = vec![0u8; lowest_mipmap_size * 16];
    iwi_file.read_exact(&mut texture)?;

    // Create output directory if needed
    if let Some(parent) = dds_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write DDS file
    let mut dds_file = std::fs::File::create(dds_path)?;

    // Write DDS header
    let dds_header = DdsHeader::from_iwi(&header);
    dds_file.write_all(bytemuck::bytes_of(&dds_header))?;

    // Write texture data (main texture first, then mipmaps)
    dds_file.write_all(&texture)?;
    dds_file.write_all(&mipmap1)?;
    dds_file.write_all(&mipmap2)?;

    Ok(())
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iwi_header_size() {
        // Verify header size matches expected
        assert_eq!(std::mem::size_of::<IwiHeader>(), 24);
    }

    #[test]
    fn test_dds_header_size() {
        // DDS header should be 128 bytes (4 magic + 124 header)
        assert_eq!(std::mem::size_of::<DdsHeader>(), 128);
    }

    #[test]
    fn test_dxt_fourcc() {
        let header = IwiHeader {
            magic: *b"IWi\x06",
            dxt: IwiHeader::DXT1,
            flags: 0,
            width: 256,
            height: 256,
            file_size: 0,
            mipmap2_offset: 0,
            mipmap1_offset: 0,
            texture_offset: 0,
        };

        assert_eq!(&header.dxt_fourcc(), b"DXT1");
    }
}
