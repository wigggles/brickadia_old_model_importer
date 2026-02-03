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

    /// Get the engine folder name for this game source's textures/config.
    /// Used for locating game-specific material mapping configs.
    /// Structure: data/game_textures/<engine>/<game>/
    pub fn folder_name(&self) -> &'static str {
        match self {
            GameSource::Auto => "auto",
            GameSource::HalfLife1 => "goldsrc",      // GoldSrc engine (Valve)
            GameSource::Quake1 => "idtech",          // id Tech engine (id Software)
            GameSource::Quake2 => "idtech",          // id Tech engine (id Software)
            GameSource::CallOfDuty1 => "iw",         // IW engine (Infinity Ward)
            GameSource::CallOfDuty2 => "iw",         // IW engine (Infinity Ward)
            GameSource::MedalOfHonor => "idtech",    // id Tech engine (id Software)
        }
    }

    /// Get the game subfolder name for this game source.
    /// Used with folder_name() to build full path: <engine>/<game>/
    pub fn game_name(&self) -> &'static str {
        match self {
            GameSource::Auto => "auto",
            GameSource::HalfLife1 => "halflife",
            GameSource::Quake1 => "quake",
            GameSource::Quake2 => "quake2",
            GameSource::CallOfDuty1 => "cod1",
            GameSource::CallOfDuty2 => "cod2",
            GameSource::MedalOfHonor => "mohaa",
        }
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

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

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
    convert_bsp_to_obj_with_textures(bsp_path, output_dir, None::<&Path>)
}

/// Convert a BSP file to OBJ format with optional external texture directory.
///
/// If `texture_dir` is provided, external textures (VTF files) will be loaded
/// from that directory to fill in textures not embedded in the BSP.
///
/// # Arguments
///
/// * `bsp_path` - Path to the input BSP file.
/// * `output_dir` - Directory where OBJ, MTL, and textures will be written.
/// * `texture_dir` - Optional path to directory containing VTF texture files.
pub fn convert_bsp_to_obj_with_textures<P: AsRef<Path>, Q: AsRef<Path>, R: AsRef<Path>>(
    bsp_path: P,
    output_dir: Q,
    texture_dir: Option<R>,
) -> BspResult<()> {
    let bsp_path = bsp_path.as_ref();
    let output_dir = output_dir.as_ref();
    let texture_dir = texture_dir.as_ref().map(|p| p.as_ref());

    // Step 1: Detect format — GoldSrc has no "IBSP" signature
    if let Some(goldsrc_version) = bsp::goldsrc::detect_goldsrc_version(bsp_path)? {
        // GoldSrc/Quake/Half-Life format
        match goldsrc_version {
            bsp::goldsrc::VERSION_HALFLIFE | bsp::goldsrc::VERSION_QUAKE1 => {
                // Parse GoldSrc BSP (no external palette for HL1 — it's embedded)
                let mut bsp_data = bsp::goldsrc::parse(bsp_path, None)?;
                
                // Load external textures from VTF files if texture_dir provided
                if let Some(tex_dir) = texture_dir {
                    load_external_textures(&mut bsp_data, tex_dir);
                }
                
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

/// Load external textures from VTF files for materials with empty pixel data.
fn load_external_textures(bsp_data: &mut bsp::BspData, texture_dir: &Path) {
    for texture in &mut bsp_data.textures {
        // Skip if already has pixel data
        if !texture.pixels.is_empty() {
            continue;
        }
        
        // Try to find VTF file
        if let Some(vtf) = texture::vtf::find_texture(&texture.name, texture_dir) {
            texture.width = vtf.width;
            texture.height = vtf.height;
            texture.pixels = vtf.pixels;
        }
    }
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
    convert_bsp_to_obj_with_game_and_textures(bsp_path, output_dir, game, None::<&Path>)
}

/// Convert a BSP file to OBJ format with explicit game source and texture directory.
///
/// # Arguments
///
/// * `bsp_path` - Path to the input BSP file.
/// * `output_dir` - Directory where OBJ, MTL, and textures will be written.
/// * `game` - The game source to use. If `GameSource::Auto`, auto-detection is used.
/// * `texture_dir` - Optional path to directory containing VTF texture files.
pub fn convert_bsp_to_obj_with_game_and_textures<P: AsRef<Path>, Q: AsRef<Path>, R: AsRef<Path>>(
    bsp_path: P,
    output_dir: Q,
    game: GameSource,
    texture_dir: Option<R>,
) -> BspResult<()> {
    let bsp_path = bsp_path.as_ref();
    let output_dir = output_dir.as_ref();
    let texture_dir = texture_dir.as_ref().map(|p| p.as_ref());

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
            let mut bsp_data = bsp::goldsrc::parse(bsp_path, None)?;
            
            // Load external textures from VTF files if texture_dir provided
            if let Some(tex_dir) = texture_dir {
                load_external_textures(&mut bsp_data, tex_dir);
            }
            
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
