//! # Medal of Honor: Allied Assault BSP Format
//!
//! This module implements parsing for MoHAA BSP files (versions 18, 19, 21).
//!
//! ## Status
//!
//! **PLACEHOLDER** — This module is not yet implemented.
//!
//! ## Version numbers
//!
//! - 18 = Beta Allied Assault
//! - 19 = Release Allied Assault
//! - 21 = Breakthrough expansion
//!
//! ## Differences from CoD formats
//!
//! MoHAA uses different lump structures. See the C# reference:
//! - `MoHAA_Lumps.cs` for lump definitions
//! - `MoHAABSP.cs` for parsing logic
//!
//! ## TODO
//!
//! - [ ] Implement MoHAA lump definitions
//! - [ ] Implement MoHAA vertex/face structs
//! - [ ] Implement parse function
//! - [ ] Test with real MoHAA BSP files

use std::path::Path;

use crate::bsp::BspData;
use crate::{BspError, BspResult};

// -----------------------------------------------------------------------------
// MoHAA-specific structs (placeholder)
// -----------------------------------------------------------------------------

// TODO: Implement MoHAA-specific structs
//
// See C# reference in submodules/bsp-converter-obj_textured/Decompiler/Lumps/MoHAA_Lumps.cs

// -----------------------------------------------------------------------------
// Parsing
// -----------------------------------------------------------------------------

/// Parse a MoHAA BSP file.
///
/// # Status
///
/// **NOT IMPLEMENTED** — Returns an error.
///
/// # Arguments
///
/// * `path` - Path to the BSP file.
/// * `version` - The BSP version (18, 19, or 21).
pub fn parse<P: AsRef<Path>>(_path: P, version: i32) -> BspResult<BspData> {
    // TODO: Implement MoHAA parsing
    Err(BspError::UnsupportedVersion(version))
}
