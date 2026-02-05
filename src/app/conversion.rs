//! Conversion trigger and orchestration code.
//!
//! Contains the methods that initiate and manage the conversion process.

use rfd::{MessageDialog, MessageLevel};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use super::logger;
use crate::conversion::{material_mapping, perform_conversion};
use crate::util::validate_obj_resources;

use super::types::*;

impl Obj2Brz {
    pub fn do_conversion(&mut self) {
        // Handle BSP files: convert to OBJ first
        if self.input_file_type == InputFileType::Bsp {
            self.do_bsp_conversion();
            return;
        }

        // OBJ file: validate resources before conversion
        let missing = match validate_obj_resources(&self.input_file_path) {
            Ok(m) => m,
            Err(e) => {
                MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_title("Conversion Error")
                    .set_description(&format!("{}", e))
                    .show();
                return;
            }
        };

        // If there are missing resources, ask user what to do
        if missing.has_issues() {
            let message = format!("The following issues were found:\n\n{}", missing.description());
            self.missing_resources_dialog = Some(message);
            return;
        }

        // No missing resources, continue with conversion
        self.continue_conversion(false);
    }

    pub fn do_bsp_conversion(&mut self) {
        self.conversion_in_progress = true;
        self.conversion_start_time = Some(std::time::Instant::now());
        self.conversion_progress.store(0, std::sync::atomic::Ordering::Relaxed);
        self.conversion_cancelled.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut stage) = self.conversion_stage.lock() {
            *stage = "Starting BSP conversion...".to_string();
        }
        self.logger.log("Starting BSP to BRZ conversion...".to_string());

        // Create channel to signal completion
        let (tx, rx) = mpsc::channel();
        self.conversion_done_receiver = Some(rx);

        // Clone data needed for the background thread
        let input_file_path = self.input_file_path.clone();
        let output_directory = self.output_directory.clone();
        let save_name = self.save_name.clone();
        let save_owner_name = self.save_owner_name.clone();
        let scale = self.scale;
        let bricktype = self.bricktype;
        let simplify = self.simplify;
        let split_by_material = self.split_by_material;
        let grid_offset_x = self.grid_offset_x;
        let grid_offset_y = self.grid_offset_y;
        let grid_offset_z = self.grid_offset_z;
        let rotation_x = self.rotation_x;
        let rotation_y = self.rotation_y;
        let rotation_z = self.rotation_z;
        let show_origin_marker = self.show_origin_marker;
        let scale_x = self.scale_x;
        let scale_y = self.scale_y;
        let scale_z = self.scale_z;
        let use_material_mapping = self.use_material_mapping;
        let group_by_brick_material = self.group_by_brick_material;
        let use_smooth_bricks = self.use_smooth_bricks;
        let color_merge_threshold = self.color_merge_threshold;
        let single_color = self.single_color;
        let save_description = self.save_description.clone();
        let detected_game_source = self.detected_game_source;
        let match_brickadia_colorset = self.match_brickadia_colorset;
        let color_mode = self.color_mode;
        let brick_scale = self.brick_scale;
        let material = self.material;
        let material_intensity = self.material_intensity;
        let bsp_game_source = self.bsp_game_source;
        let logger = self.logger.clone();
        let progress = self.conversion_progress.clone();
        let stage = self.conversion_stage.clone();
        let cancelled = self.conversion_cancelled.clone();

        // Spawn background thread for BSP conversion
        thread::spawn(move || {
            // Helper to set progress
            let set_prog = |p: u32, s: &str| {
                progress.store(p, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut st) = stage.lock() {
                    *st = s.to_string();
                }
            };

            // Step 1: Convert BSP to OBJ in a temp directory
            set_prog(5, &format!("Converting BSP ({})...", bsp_game_source.display_name()));
            logger.log(format!("Converting BSP using {} format...", bsp_game_source.display_name()));

            // Create temp directory for OBJ output
            // In debug mode, use data/exports/bsp_temp for easier inspection
            // In release mode, use system temp directory
            let temp_dir = if crate::DEBUG_MODE {
                logger::get_exports_dir().join("bsp_temp")
            } else {
                std::env::temp_dir().join("obj2brz_bsp_temp")
            };
            if let Err(e) = std::fs::create_dir_all(&temp_dir) {
                logger.log(format!("Error creating temp directory: {}", e));
                let _ = tx.send(());
                return;
            }
            if crate::DEBUG_MODE {
                logger.log(format!("[DEBUG] BSP temp directory: {}", temp_dir.display()));
            }

            // Convert BSP to OBJ
            // Check for external texture directory based on game source
            let texture_dir_opt = material_mapping::MaterialMapping::get_assets_dir(bsp_game_source)
                .and_then(|path| {
                    if path.exists() {
                        let has_files = std::fs::read_dir(&path)
                            .map(|mut entries| entries.next().is_some())
                            .unwrap_or(false);
                        if has_files {
                            logger.log(format!("Using external textures from: {}", path.display()));
                            Some(path)
                        } else {
                            logger.log(format!("Assets directory exists but is empty: {}", path.display()));
                            None
                        }
                    } else {
                        None
                    }
                });
            
            if texture_dir_opt.is_none() {
                logger.log("No external texture directory found, using embedded/inferred colors".to_string());
            }
            
            let bsp_result = bsp_converter::convert_bsp_to_obj_with_game_and_textures(
                &input_file_path,
                &temp_dir,
                bsp_game_source,
                texture_dir_opt.as_ref().map(|p| p.as_path()),
            );

            if let Err(e) = bsp_result {
                logger.log(format!("BSP conversion error: {}", e));
                MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_title("BSP Conversion Failed")
                    .set_description(&format!("{}", e))
                    .show();
                let _ = tx.send(());
                return;
            }

            set_prog(20, "BSP converted, loading OBJ...");
            logger.log("BSP converted to OBJ successfully".to_string());

            // Find the generated OBJ file
            let bsp_path = Path::new(&input_file_path);
            let map_name = bsp_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("map");
            let obj_path = temp_dir.join(format!("{}.obj", map_name));

            if !obj_path.exists() {
                logger.log("Error: Generated OBJ file not found".to_string());
                let _ = tx.send(());
                return;
            }

            let obj_path_str = obj_path.to_string_lossy().to_string();

            // Step 2: Now convert OBJ to BRZ using the normal pipeline
            set_prog(25, "Converting OBJ to BRZ...");
            logger.log("Converting OBJ to BRZ...".to_string());

            let opts = Obj2Brz {
                bricktype,
                brick_scale,
                input_file_path_receiver: None,
                input_file_path: obj_path_str,
                match_brickadia_colorset,
                color_mode,
                material,
                material_intensity,
                output_directory_receiver: None,
                output_directory,
                save_owner_id: "d66c4ad5-59fc-4a9b-80b8-08dedc25bff9".into(),
                save_owner_name,
                save_name,
                scale,
                simplify,
                split_by_material,
                grid_offset_x,
                grid_offset_y,
                grid_offset_z,
                rotation_x,
                rotation_y,
                rotation_z,
                show_origin_marker,
                scale_x,
                scale_y,
                scale_z,
                use_material_mapping,
                group_by_brick_material,
                use_smooth_bricks,
                color_merge_threshold,
                single_color,
                save_description: save_description.clone(),
                missing_resources_dialog: None,
                pending_conversion_skip_textures: false,
                logger: logger.clone(),
                conversion_in_progress: true,
                conversion_done_receiver: None,
                conversion_progress: progress.clone(),
                conversion_stage: stage.clone(),
                conversion_cancelled: cancelled.clone(),
                conversion_start_time: None,
                input_file_type: InputFileType::Obj,
                bsp_game_source: bsp_game_source,
                detected_game_source: detected_game_source,
                profiles: Vec::new(),
                selected_profile_index: None,
                new_profile_name: String::new(),
                help_lossy_expanded: false,
                help_color_merge_expanded: false,
                help_scale_expanded: false,
                help_bricktype_expanded: false,
                help_color_mode_expanded: false,
                help_surface_expanded: false,
            };

            // Load textures from BSP-converted OBJs (PNG textures exported from VTF files)
            if let Err(e) = perform_conversion(&opts, false) {
                logger.log(format!("Error: {}", e));
                MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_title("Conversion Failed")
                    .set_description(&format!("{}", e))
                    .show();
            }

            // Cleanup temp directory (optional, leave for debugging)
            // let _ = std::fs::remove_dir_all(&temp_dir);

            // Signal completion
            set_prog(100, "Complete!");
            let _ = tx.send(());
        });
    }

    pub fn continue_conversion(&mut self, skip_textures: bool) {
        self.conversion_in_progress = true;
        self.conversion_start_time = Some(std::time::Instant::now());
        self.conversion_progress.store(0, std::sync::atomic::Ordering::Relaxed);
        self.conversion_cancelled.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut stage) = self.conversion_stage.lock() {
            *stage = "Initializing...".to_string();
        }
        self.logger.log("Starting conversion...".to_string());

        // Create channel to signal completion
        let (tx, rx) = mpsc::channel();
        self.conversion_done_receiver = Some(rx);

        // Clone data needed for the background thread
        let input_file_path = self.input_file_path.clone();
        let output_directory = self.output_directory.clone();
        let save_name = self.save_name.clone();
        let save_owner_name = self.save_owner_name.clone();
        let scale = self.scale;
        let bricktype = self.bricktype;
        let simplify = self.simplify;
        let split_by_material = self.split_by_material;
        let grid_offset_x = self.grid_offset_x;
        let grid_offset_y = self.grid_offset_y;
        let grid_offset_z = self.grid_offset_z;
        let rotation_x = self.rotation_x;
        let rotation_y = self.rotation_y;
        let rotation_z = self.rotation_z;
        let show_origin_marker = self.show_origin_marker;
        let scale_x = self.scale_x;
        let scale_y = self.scale_y;
        let scale_z = self.scale_z;
        let use_material_mapping = self.use_material_mapping;
        let group_by_brick_material = self.group_by_brick_material;
        let use_smooth_bricks = self.use_smooth_bricks;
        let color_merge_threshold = self.color_merge_threshold;
        let single_color = self.single_color;
        let save_description = self.save_description.clone();
        let bsp_game_source = self.bsp_game_source;
        let detected_game_source = self.detected_game_source;
        let match_brickadia_colorset = self.match_brickadia_colorset;
        let color_mode = self.color_mode;
        let brick_scale = self.brick_scale;
        let material = self.material;
        let material_intensity = self.material_intensity;
        let logger = self.logger.clone();
        let progress = self.conversion_progress.clone();
        let stage = self.conversion_stage.clone();
        let cancelled = self.conversion_cancelled.clone();

        // Spawn background thread for conversion
        thread::spawn(move || {
            // Create a minimal Obj2Brz for the conversion functions
            let opts = Obj2Brz {
                bricktype,
                brick_scale,
                input_file_path_receiver: None,
                input_file_path,
                match_brickadia_colorset,
                color_mode,
                material,
                material_intensity,
                output_directory_receiver: None,
                output_directory,
                save_owner_id: "d66c4ad5-59fc-4a9b-80b8-08dedc25bff9".into(),
                save_owner_name,
                save_name,
                scale,
                simplify,
                split_by_material,
                grid_offset_x,
                grid_offset_y,
                grid_offset_z,
                rotation_x,
                rotation_y,
                rotation_z,
                show_origin_marker,
                scale_x,
                scale_y,
                scale_z,
                use_material_mapping,
                group_by_brick_material,
                use_smooth_bricks,
                color_merge_threshold,
                single_color,
                save_description,
                missing_resources_dialog: None,
                pending_conversion_skip_textures: false,
                logger: logger.clone(),
                conversion_in_progress: true,
                conversion_done_receiver: None,
                conversion_progress: progress.clone(),
                conversion_stage: stage.clone(),
                conversion_cancelled: cancelled.clone(),
                conversion_start_time: None,
                // BSP fields
                input_file_type: InputFileType::Obj,
                bsp_game_source,
                detected_game_source,
                profiles: Vec::new(),
                selected_profile_index: None,
                new_profile_name: String::new(),
                help_lossy_expanded: false,
                help_color_merge_expanded: false,
                help_scale_expanded: false,
                help_bricktype_expanded: false,
                help_color_mode_expanded: false,
                help_surface_expanded: false,
            };

            if let Err(e) = perform_conversion(&opts, skip_textures) {
                logger.log(format!("Error: {}", e));
                MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_title("Conversion Failed")
                    .set_description(&format!("{}", e))
                    .show();
            }

            // Signal completion
            progress.store(100, std::sync::atomic::Ordering::Relaxed);
            let _ = tx.send(());
        });
    }
}
