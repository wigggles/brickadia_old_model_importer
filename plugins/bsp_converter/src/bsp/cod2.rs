//! # Call of Duty 2 BSP Format
//!
//! This module implements parsing for CoD2 BSP files (version 4).
//!
//! ## Status
//!
//! **PLACEHOLDER** — This module is not yet implemented.
//!
//! ## Differences from CoD1
//!
//! CoD2 uses a different vertex format with:
//! - Different field ordering (normal before UV)
//! - RGBA color per vertex
//! - Additional unknown fields
//!
//! ## TODO
//!
//! - [ ] Implement Cod2Vertex struct
//! - [ ] Implement parse function
//! - [ ] Test with real CoD2 BSP files

use std::path::Path;

use crate::bsp::BspData;
use crate::{BspError, BspResult};

// -----------------------------------------------------------------------------
// CoD2-specific structs (placeholder)
// -----------------------------------------------------------------------------

// TODO: Implement CoD2 vertex format
//
// Based on C# reference:
// ```csharp
// public struct CoD2Vertex {
//     public float[] Position;  // [3]
//     public float[] Normal;    // [3]
//     public char[] RGBa;       // [4]
//     public float[] UV;        // [2]
//     public float[] ST;        // [2] (lightmap UVs?)
//     public float[] Unknown;   // [6]
// }
// ```

// -----------------------------------------------------------------------------
// Parsing
// -----------------------------------------------------------------------------

/// Parse a CoD2 BSP file.
///
/// # Status
///
/// **NOT IMPLEMENTED** — Returns an error.
///
/// # Arguments
///
/// * `path` - Path to the BSP file.
pub fn parse<P: AsRef<Path>>(_path: P) -> BspResult<BspData> {
    // TODO: Implement CoD2 parsing
    Err(BspError::UnsupportedVersion(4))
}
