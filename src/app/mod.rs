//! Application state and UI module.
//!
//! This module contains the main application struct (`Obj2Brz`) and all UI-related code.
//! It separates the GUI concerns from the conversion logic.

mod conversion;
pub mod gui;
mod icon;
pub mod logger;
mod types;
mod ui;

pub use icon::ICON;
pub use logger::{Logger, get_cache_dir, get_data_dir, load_preview_icon};
pub use types::*;
pub use ui::*;
