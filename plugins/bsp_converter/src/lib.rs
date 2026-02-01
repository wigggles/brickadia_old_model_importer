//! # BSP Converter
//!
//! A Rust library for converting BSP (Binary Space Partitioning) map files
//! to Wavefront OBJ format, with optional texture extraction.
//!
//! ## Supported formats
//!
//! - **Half-Life 1 (GoldSrc)** — version 30 (PRIORITY)
//! - **Quake 1** — version 29
//! - **CoD1 BSP** — version 59
//! - **CoD2 BSP** — version 4 (placeholder)
//! - **MoHAA BSP** — versions 18, 19, 21 (placeholder)
//!
//! ## Usage
//!
//! ```ignore
//! use bsp_converter::{convert_bsp_to_obj, detect_game_source, GameSource};
//!
//! // Auto-detect game source
//! let game = detect_game_source("path/to/map.bsp")?;
//! println!("Detected: {:?}", game);
//!
//! // Convert with auto-detection
//! convert_bsp_to_obj("path/to/map.bsp", "output/folder")?;
//!
//! // Or convert with explicit game source
//! convert_bsp_to_obj_with_game("path/to/map.bsp", "output/folder", GameSource::HalfLife1)?;
//! ```
//!
//! ## Architecture
//!
//! See `DESIGN.md` for detailed design notes.

pub mod binary;
pub mod bsp;
pub mod obj;
pub mod texture;

use std::path::Path;
use thiserror::Error;

// -----------------------------------------------------------------------------
// Game Source Enum (for GUI integration)
// -----------------------------------------------------------------------------

/// Supported game sources for BSP conversion.
///
/// This enum is designed for GUI integration — it can be displayed in a
/// dropdown/combo box to let users select which game format to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameSource {
    /// Auto-detect based on file header.
    #[default]
    Auto,

    /// Half-Life 1 (GoldSrc engine) — BSP version 30.
    /// This is the **primary target** for this converter.
    HalfLife1,

    /// Quake 1 — BSP version 29.
    Quake1,

    /// Quake 2 — BSP version 38 (has IBSP signature).
    Quake2,

    /// Call of Duty 1 — BSP version 59 (IBSP signature).
    CallOfDuty1,

    /// Call of Duty 2 — BSP version 4 (IBSP signature).
    CallOfDuty2,

    /// Medal of Honor: Allied Assault — BSP versions 18, 19, 21.
    MedalOfHonor,
}

impl GameSource {
    /// Get all available game sources (excluding Auto).
    pub fn all() -> &'static [GameSource] {
        &[
            GameSource::HalfLife1,
            GameSource::Quake1,
            GameSource::Quake2,
            GameSource::CallOfDuty1,
            GameSource::CallOfDuty2,
            GameSource::MedalOfHonor,
        ]
    }

    /// Get a human-readable display name for this game source.
    pub fn display_name(&self) -> &'static str {
        match self {
            GameSource::Auto => "Auto-detect",
            GameSource::HalfLife1 => "Half-Life 1 (GoldSrc)",
            GameSource::Quake1 => "Quake 1",
            GameSource::Quake2 => "Quake 2",
            GameSource::CallOfDuty1 => "Call of Duty 1",
            GameSource::CallOfDuty2 => "Call of Duty 2",
            GameSource::MedalOfHonor => "Medal of Honor: Allied Assault",
        }
    }

    /// Check if this game source is currently implemented.
    pub fn is_implemented(&self) -> bool {
        matches!(
            self,
            GameSource::Auto | GameSource::HalfLife1 | GameSource::Quake1 | GameSource::CallOfDuty1
        )
    }
}

impl std::fmt::Display for GameSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// -----------------------------------------------------------------------------
// Error types
// -----------------------------------------------------------------------------

/// Errors that can occur during BSP conversion.
#[derive(Error, Debug)]
pub enum BspError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unsupported BSP version: {0}")]
    UnsupportedVersion(i32),

    #[error("Invalid BSP file: {0}")]
    InvalidFile(String),

    #[error("Texture extraction error: {0}")]
    TextureError(String),

    #[error("Game source not implemented: {0}")]
    NotImplemented(String),
}

/// Result type alias for BSP operations.
pub type BspResult<T> = Result<T, BspError>;

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Convert a BSP file to OBJ format.
///
/// Automatically detects the BSP format (GoldSrc/Quake/Half-Life vs CoD/id Tech 3)
/// and dispatches to the appropriate parser.
///
/// # Arguments
///
/// * `bsp_path` - Path to the input BSP file.
/// * `output_dir` - Directory where OBJ, MTL, and textures will be written.
///
/// # Returns
///
/// Returns `Ok(())` on success, or a `BspError` on failure.
///
/// # Supported formats
///
/// - **Half-Life 1 (GoldSrc)** — version 30 (PRIORITY)
/// - **Quake 1** — version 29
/// - **Call of Duty 1** — version 59 (IBSP signature)
/// - **Call of Duty 2** — version 4 (placeholder)
/// - **MoHAA** — versions 18, 19, 21 (placeholder)
///
/// # Example
///
/// ```ignore
/// use bsp_converter::convert_bsp_to_obj;
///
/// // Half-Life map
/// convert_bsp_to_obj("maps/c1a0.bsp", "output/")?;
///
/// // CoD map
/// convert_bsp_to_obj("maps/mp_harbor.bsp", "output/")?;
/// ```
pub fn convert_bsp_to_obj<P: AsRef<Path>, Q: AsRef<Path>>(
    bsp_path: P,
    output_dir: Q,
) -> BspResult<()> {
    let bsp_path = bsp_path.as_ref();
    let output_dir = output_dir.as_ref();

    // Step 1: Detect format — GoldSrc has no "IBSP" signature
    if let Some(goldsrc_version) = bsp::goldsrc::detect_goldsrc_version(bsp_path)? {
        // GoldSrc/Quake/Half-Life format
        match goldsrc_version {
            bsp::goldsrc::VERSION_HALFLIFE | bsp::goldsrc::VERSION_QUAKE1 => {
                // Parse GoldSrc BSP (no external palette for HL1 — it's embedded)
                let bsp_data = bsp::goldsrc::parse(bsp_path, None)?;
                obj::writer::write_obj(&bsp_data, output_dir)?;
                obj::mtl::write_mtl(&bsp_data, output_dir)?;
            }
            v => {
                return Err(BspError::UnsupportedVersion(v));
            }
        }
    } else {
        // id Tech 3 / CoD format (has "IBSP" signature)
        let header = bsp::header::read_header(bsp_path)?;

        match header.version {
            59 => {
                // CoD1 BSP
                let bsp_data = bsp::cod1::parse(bsp_path)?;
                obj::writer::write_obj(&bsp_data, output_dir)?;
                obj::mtl::write_mtl(&bsp_data, output_dir)?;
            }
            4 => {
                // CoD2 BSP — placeholder
                return Err(BspError::UnsupportedVersion(4));
            }
            18 | 19 | 21 => {
                // MoHAA BSP — placeholder
                return Err(BspError::UnsupportedVersion(header.version));
            }
            v => {
                return Err(BspError::UnsupportedVersion(v));
            }
        }
    }

    Ok(())
}

/// Detect the game source from a BSP file.
///
/// Reads the file header and determines which game format it is.
/// This is useful for GUI integration to show the detected format
/// before conversion.
///
/// # Arguments
///
/// * `bsp_path` - Path to the BSP file.
///
/// # Returns
///
/// The detected `GameSource`, or an error if the file cannot be read.
///
/// # Example
///
/// ```ignore
/// use bsp_converter::{detect_game_source, GameSource};
///
/// let game = detect_game_source("maps/c1a0.bsp")?;
/// match game {
///     GameSource::HalfLife1 => println!("Half-Life 1 map detected!"),
///     GameSource::CallOfDuty1 => println!("CoD1 map detected!"),
///     _ => println!("Detected: {}", game),
/// }
/// ```
pub fn detect_game_source<P: AsRef<Path>>(bsp_path: P) -> BspResult<GameSource> {
    let bsp_path = bsp_path.as_ref();

    // Try GoldSrc detection first (no IBSP signature)
    if let Some(version) = bsp::goldsrc::detect_goldsrc_version(bsp_path)? {
        return Ok(match version {
            bsp::goldsrc::VERSION_HALFLIFE => GameSource::HalfLife1,
            bsp::goldsrc::VERSION_QUAKE1 => GameSource::Quake1,
            bsp::goldsrc::VERSION_QUAKE2 => GameSource::Quake2,
            _ => GameSource::Auto, // Unknown GoldSrc version
        });
    }

    // Has IBSP signature — id Tech 3 / CoD format
    let header = bsp::header::read_header(bsp_path)?;

    Ok(match header.version {
        59 => GameSource::CallOfDuty1,
        4 => GameSource::CallOfDuty2,
        18 | 19 | 21 => GameSource::MedalOfHonor,
        _ => GameSource::Auto, // Unknown IBSP version
    })
}

/// Convert a BSP file to OBJ format with an explicit game source.
///
/// Use this when you want to override auto-detection, for example
/// when the user has selected a specific game from a dropdown.
///
/// # Arguments
///
/// * `bsp_path` - Path to the input BSP file.
/// * `output_dir` - Directory where OBJ, MTL, and textures will be written.
/// * `game` - The game source to use. If `GameSource::Auto`, auto-detection is used.
///
/// # Returns
///
/// Returns `Ok(())` on success, or a `BspError` on failure.
pub fn convert_bsp_to_obj_with_game<P: AsRef<Path>, Q: AsRef<Path>>(
    bsp_path: P,
    output_dir: Q,
    game: GameSource,
) -> BspResult<()> {
    let bsp_path = bsp_path.as_ref();
    let output_dir = output_dir.as_ref();

    // If Auto, use auto-detection
    let game = if game == GameSource::Auto {
        detect_game_source(bsp_path)?
    } else {
        game
    };

    // Check if implemented
    if !game.is_implemented() {
        return Err(BspError::NotImplemented(game.display_name().to_string()));
    }

    // Dispatch to appropriate parser
    match game {
        GameSource::HalfLife1 | GameSource::Quake1 => {
            let bsp_data = bsp::goldsrc::parse(bsp_path, None)?;
            obj::writer::write_obj(&bsp_data, output_dir)?;
            obj::mtl::write_mtl(&bsp_data, output_dir)?;
        }
        GameSource::CallOfDuty1 => {
            let bsp_data = bsp::cod1::parse(bsp_path)?;
            obj::writer::write_obj(&bsp_data, output_dir)?;
            obj::mtl::write_mtl(&bsp_data, output_dir)?;
        }
        GameSource::Quake2 => {
            return Err(BspError::NotImplemented("Quake 2".to_string()));
        }
        GameSource::CallOfDuty2 => {
            return Err(BspError::NotImplemented("Call of Duty 2".to_string()));
        }
        GameSource::MedalOfHonor => {
            return Err(BspError::NotImplemented("Medal of Honor".to_string()));
        }
        GameSource::Auto => {
            // Should not reach here, but handle gracefully
            return convert_bsp_to_obj(bsp_path, output_dir);
        }
    }

    Ok(())
}

/// Check if a file path looks like a BSP file (by extension).
///
/// This is a simple helper for GUI integration to determine
/// whether to show BSP-specific options.
pub fn is_bsp_file<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref()
        .extension()
        .map(|ext| ext.eq_ignore_ascii_case("bsp"))
        .unwrap_or(false)
}

/// Check if a file path looks like an OBJ file (by extension).
pub fn is_obj_file<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref()
        .extension()
        .map(|ext| ext.eq_ignore_ascii_case("obj"))
        .unwrap_or(false)
}
