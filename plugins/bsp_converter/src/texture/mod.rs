//! # Texture Extraction Module
//!
//! This module handles extracting textures from game archives (PK3/ZIP)
//! and converting proprietary formats (IWI) to standard formats (DDS).
//!
//! ## Submodules
//!
//! - `pk3` — PK3/ZIP archive reading
//! - `iwi` — IWI to DDS texture conversion

pub mod pk3;
pub mod iwi;
