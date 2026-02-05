//! obj2brz - Convert OBJ/BSP files to Brickadia BRZ saves
//!
//! This is the main entry point for the application. The actual logic is
//! organized into modules:
//!
//! - `app/` - Application state, UI, logging, and conversion triggers
//! - `conversion/` - Pipeline, material mapping, brick alignment, grid assembly
//! - `voxel/` - Voxelization algorithms and octree data structure
//! - `simplify/` - Brick simplification algorithms
//! - `output/` - BRZ file writing
//! - `color/` - Color utilities and palette
//! - `geometry/` - Geometric calculations
//! - `error` - Error types
//! - `util` - Utility functions

mod app;
mod color;
mod conversion;
mod error;
mod geometry;
mod output;
mod simplify;
mod util;
mod voxel;

use eframe::{egui, run_native, NativeOptions};
use std::env;

use app::{Obj2Brz, Logger, get_data_dir, get_cache_dir, ICON, WINDOW_WIDTH, WINDOW_HEIGHT};

/// Re-export DEBUG_MODE for use in other modules
pub use util::DEBUG_MODE;

fn main() {
    let logger = Logger::new();
    logger.log("obj2brz started - ready to convert OBJ files to Brickadia saves".to_string());

    // Log the data directory location
    let data_dir = get_data_dir();
    let cache_dir = get_cache_dir();
    logger.log(format!("Data directory: {}", data_dir.display()));
    logger.log(format!("User cache: {}", cache_dir.display()));

    let build_dir = match env::consts::OS {
        "windows" => {
            dirs::data_local_dir()
                .map(|p| p.join("Brickadia").join("Saved").join("Builds"))
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "builds".to_string())
        }
        "linux" => {
            dirs::config_dir()
                .map(|p| p.join("Epic").join("Brickadia").join("Saved").join("Builds"))
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "builds".to_string())
        }
        _ => "builds".to_string(),
    };

    let build_dir_clone = build_dir.clone();
    
    // Use our custom cache directory for eframe persistence
    let persistence_path = get_cache_dir().join("app_state");
    
    let win_option = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
            .with_min_inner_size([500.0, 400.0])
            .with_resizable(true)
            .with_icon(egui::IconData {
                rgba: ICON.to_vec(),
                width: 32,
                height: 32,
            }),
        persistence_path: Some(persistence_path),
        ..Default::default()
    };
    let _ = run_native(
        "obj2brz",
        win_option,
        Box::new(move |cc| {
            // Load previous state if available
            let mut app = if let Some(storage) = cc.storage {
                eframe::get_value(storage, eframe::APP_KEY).unwrap_or_else(|| Obj2Brz {
                    output_directory: build_dir_clone.clone(),
                    logger: logger.clone(),
                    ..Default::default()
                })
            } else {
                Obj2Brz {
                    output_directory: build_dir_clone.clone(),
                    logger: logger.clone(),
                    ..Default::default()
                }
            };

            // Always re-initialize transient fields
            app.logger = logger.clone();
            app.conversion_in_progress = false;
            app.input_file_path_receiver = None;
            app.output_directory_receiver = None;
            app.conversion_done_receiver = None;
            app.missing_resources_dialog = None;
            app.pending_conversion_skip_textures = false;
            app.conversion_progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
            app.conversion_stage = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            app.conversion_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

            Ok(Box::new(app))
        }),
    );
}
