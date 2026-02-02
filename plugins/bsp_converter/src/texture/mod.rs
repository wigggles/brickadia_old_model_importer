//! # Texture Extraction Module
//!
//! This module handles extracting textures from game archives (PK3/ZIP)
//! and converting proprietary formats (IWI, VTF) to standard formats.
//!
//! ## Submodules
//!
//! - `pk3` — PK3/ZIP archive reading
//! - `iwi` — IWI to DDS texture conversion
//! - `vtf` — VTF (Valve Texture Format) parsing for GoldSrc/Source textures

pub mod pk3;
pub mod iwi;
pub mod vtf;
