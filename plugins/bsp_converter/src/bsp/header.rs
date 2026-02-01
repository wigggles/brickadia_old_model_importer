//! # BSP Header Parsing
//!
//! This module handles reading and validating BSP file headers.
//!
//! ## BSP Header Format
//!
//! All supported BSP formats share a common header structure:
//! - 4-byte signature (typically "IBSP")
//! - 4-byte version number (determines which format to use)
//!
//! ## Version numbers
//!
//! - 59 = Call of Duty 1
//! - 4 = Call of Duty 2
//! - 18 = Medal of Honor: Allied Assault (Beta)
//! - 19 = Medal of Honor: Allied Assault (Release)
//! - 21 = Medal of Honor: Breakthrough

use bytemuck::{Pod, Zeroable};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::binary::read_struct;
use crate::{BspError, BspResult};

// -----------------------------------------------------------------------------
// Header struct
// -----------------------------------------------------------------------------

/// Raw BSP file header as stored on disk.
///
/// This is a packed struct that can be read directly from the file.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct BspHeader {
    /// File signature, typically "IBSP" (0x49425350).
    pub signature: [u8; 4],

    /// BSP format version number.
    /// Determines which game format to use for parsing.
    pub version: i32,
}

impl BspHeader {
    /// Expected signature for valid BSP files.
    pub const EXPECTED_SIGNATURE: &'static [u8; 4] = b"IBSP";

    /// Check if the signature is valid.
    pub fn is_valid_signature(&self) -> bool {
        &self.signature == Self::EXPECTED_SIGNATURE
    }

    /// Get a human-readable description of the version.
    pub fn version_description(&self) -> &'static str {
        match self.version {
            59 => "Call of Duty 1",
            4 => "Call of Duty 2",
            18 => "Medal of Honor: Allied Assault (Beta)",
            19 => "Medal of Honor: Allied Assault (Release)",
            21 => "Medal of Honor: Breakthrough",
            _ => "Unknown",
        }
    }
}

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Read and validate a BSP header from a file.
///
/// # Arguments
///
/// * `path` - Path to the BSP file.
///
/// # Returns
///
/// The parsed `BspHeader`, or an error if the file is invalid.
///
/// # Errors
///
/// - `BspError::Io` if the file cannot be read.
/// - `BspError::InvalidFile` if the signature is not "IBSP".
///
/// # Example
///
/// ```ignore
/// let header = read_header("maps/mp_harbor.bsp")?;
/// println!("BSP version: {} ({})", header.version, header.version_description());
/// ```
pub fn read_header<P: AsRef<Path>>(path: P) -> BspResult<BspHeader> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read the header struct
    let header: BspHeader = read_struct(&mut reader)?;

    // Validate signature
    if !header.is_valid_signature() {
        return Err(BspError::InvalidFile(format!(
            "Invalid BSP signature: expected {:?}, got {:?}",
            BspHeader::EXPECTED_SIGNATURE,
            header.signature
        )));
    }

    Ok(header)
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size() {
        // Header should be exactly 8 bytes (4 signature + 4 version)
        assert_eq!(std::mem::size_of::<BspHeader>(), 8);
    }

    #[test]
    fn test_version_description() {
        let header = BspHeader {
            signature: *b"IBSP",
            version: 59,
        };
        assert_eq!(header.version_description(), "Call of Duty 1");
    }
}
