//! # PK3/ZIP Archive Reading
//!
//! This module handles reading textures from PK3 archives.
//!
//! ## What are PK3 files?
//!
//! PK3 files are standard ZIP archives used by id Tech 3 engine games
//! (Quake 3, Call of Duty, Medal of Honor, etc.) to store game assets.
//!
//! ## Texture locations
//!
//! Textures are typically stored under `textures/` in the archive:
//! - `textures/common/brick.tga`
//! - `textures/egypt/sand.jpg`
//!
//! ## Usage
//!
//! ```ignore
//! let archives = find_pk3_archives("C:/Games/CoD/Main")?;
//! let texture_data = extract_texture(&archives, "textures/common/brick")?;
//! ```

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::{BspError, BspResult};

// -----------------------------------------------------------------------------
// Archive discovery
// -----------------------------------------------------------------------------

/// Find all PK3 archives in a game directory.
///
/// Searches for `.pk3` files in the given directory (non-recursive).
///
/// # Arguments
///
/// * `game_dir` - Path to the game's data directory (e.g., `CoD/Main`).
///
/// # Returns
///
/// A list of paths to PK3 files.
///
/// # Example
///
/// ```ignore
/// let archives = find_pk3_archives("C:/Games/CoD/Main")?;
/// println!("Found {} archives", archives.len());
/// ```
pub fn find_pk3_archives<P: AsRef<Path>>(game_dir: P) -> BspResult<Vec<PathBuf>> {
    let game_dir = game_dir.as_ref();

    if !game_dir.is_dir() {
        return Err(BspError::TextureError(format!(
            "Game directory not found: {}",
            game_dir.display()
        )));
    }

    let mut archives = Vec::new();

    for entry in std::fs::read_dir(game_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_ascii_lowercase() == "pk3" {
                    archives.push(path);
                }
            }
        }
    }

    Ok(archives)
}

// -----------------------------------------------------------------------------
// Texture index building
// -----------------------------------------------------------------------------

/// A map from texture path (without extension) to (archive path, full entry name).
pub type TextureIndex = HashMap<String, (PathBuf, String)>;

/// Build an index of all textures in a set of PK3 archives.
///
/// This scans all archives and builds a lookup table from texture names
/// (without extension) to their location.
///
/// # Arguments
///
/// * `archives` - List of PK3 file paths to scan.
///
/// # Returns
///
/// A `TextureIndex` mapping texture names to their locations.
///
/// # Example
///
/// ```ignore
/// let archives = find_pk3_archives("C:/Games/CoD/Main")?;
/// let index = build_texture_index(&archives)?;
///
/// if let Some((archive, entry)) = index.get("textures/common/brick") {
///     println!("Found in {} as {}", archive.display(), entry);
/// }
/// ```
pub fn build_texture_index(archives: &[PathBuf]) -> BspResult<TextureIndex> {
    let mut index = TextureIndex::new();

    for archive_path in archives {
        let file = File::open(archive_path)?;
        let reader = BufReader::new(file);

        let mut archive = ZipArchive::new(reader).map_err(|e| {
            BspError::TextureError(format!("Failed to open PK3 {}: {}", archive_path.display(), e))
        })?;

        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(|e| {
                BspError::TextureError(format!("Failed to read PK3 entry: {}", e))
            })?;

            let entry_name = entry.name().to_string();

            // Only index texture files
            if entry_name.starts_with("textures/") && !entry_name.ends_with('/') {
                // Normalize path and remove extension
                let normalized = entry_name.replace('\\', "/").to_lowercase();

                // Remove extension to get the key
                let key = if let Some(dot_pos) = normalized.rfind('.') {
                    normalized[..dot_pos].to_string()
                } else {
                    normalized.clone()
                };

                // Store mapping (later archives override earlier ones)
                index.insert(key, (archive_path.clone(), entry_name));
            }
        }
    }

    Ok(index)
}

// -----------------------------------------------------------------------------
// Texture extraction
// -----------------------------------------------------------------------------

/// Extract a texture from a PK3 archive.
///
/// # Arguments
///
/// * `archive_path` - Path to the PK3 file.
/// * `entry_name` - Full name of the entry in the archive.
///
/// # Returns
///
/// The raw bytes of the texture file.
pub fn extract_texture_bytes(archive_path: &Path, entry_name: &str) -> BspResult<Vec<u8>> {
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);

    let mut archive = ZipArchive::new(reader).map_err(|e| {
        BspError::TextureError(format!("Failed to open PK3 {}: {}", archive_path.display(), e))
    })?;

    let mut entry = archive.by_name(entry_name).map_err(|e| {
        BspError::TextureError(format!("Entry {} not found: {}", entry_name, e))
    })?;

    let mut buffer = Vec::new();
    entry.read_to_end(&mut buffer)?;

    Ok(buffer)
}

/// Extract a texture to a file.
///
/// # Arguments
///
/// * `archive_path` - Path to the PK3 file.
/// * `entry_name` - Full name of the entry in the archive.
/// * `output_path` - Where to write the extracted file.
pub fn extract_texture_to_file(
    archive_path: &Path,
    entry_name: &str,
    output_path: &Path,
) -> BspResult<()> {
    let bytes = extract_texture_bytes(archive_path, entry_name)?;

    // Create parent directories if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(output_path, bytes)?;

    Ok(())
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_pk3_nonexistent() {
        let result = find_pk3_archives("/nonexistent/path");
        assert!(result.is_err());
    }
}
