mod barycentric;
mod brdb_support;
mod brick_index;
mod color;
mod color_merge;
mod error;
mod gui;
mod icon;
mod intersect;
mod logger;
mod material_mapping;
mod octree;
mod palette;
mod simplify;
mod simplify_direct;
mod voxelize;

use bsp_converter::{GameSource, is_bsp_file, detect_game_source};
use brdb::{Brick, Color, Entity};
use cgmath::Vector4;
use eframe::{egui, egui::*, run_native, App, NativeOptions};
use error::{ConversionError, ConversionResult, MissingResources};
use gui::bool_color;
use logger::Logger;
use rfd::{FileDialog, MessageDialog, MessageLevel};
use serde::{Deserialize, Serialize};
use simplify::{simplify_lossy, simplify_lossless, simplify_lossy_with_material, simplify_lossless_with_material};
use std::{
    env, io::Cursor, ops::RangeInclusive, path::Path, path::PathBuf, sync::mpsc,
    sync::mpsc::Receiver, thread,
};
use tobj::LoadOptions;
use uuid::Uuid;
use voxelize::{voxelize_with_progress, voxelize_from_pregrouped, PreGroupedTriangles, VoxelizeProgress};

// Intermediate data structure for building the save
#[derive(Clone)]
pub struct SaveData {
    pub bricks: Vec<Brick>,
    pub colors: Vec<Color>,
    pub author_name: String,
}

const WINDOW_WIDTH: f32 = 600.;
const WINDOW_HEIGHT: f32 = 700.;

/// Enable verbose debug logging throughout the conversion pipeline.
/// Set to `true` to see detailed information about each step.
const DEBUG_MODE: bool = true;

/// Enable verbose model loading logs (model names, vertex/face counts per model).
/// When false, only shows total model count summary.
const VERBOSE_MODEL_LOADING: bool = false;

/// Enable verbose texture loading logs (shows each texture being loaded).
/// When false, only shows materials that are missing textures.
const VERBOSE_TEXTURE_LOADING: bool = false;

const OBJ_ICON: &[u8; 10987] = include_bytes!("../res/obj_icon.png");

/// The type of input file being processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputFileType {
    /// Wavefront OBJ file (direct voxelization).
    Obj,
    /// BSP map file (requires conversion to OBJ first).
    Bsp,
}

impl Default for InputFileType {
    fn default() -> Self {
        InputFileType::Obj
    }
}

/// How to apply colors to bricks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorMode {
    /// Use texture colors from the model.
    TextureColors,
    /// Use a single solid color for all bricks (based on Material setting).
    SingleColor,
    /// Use material diffuse color from OBJ/MTL file.
    MaterialColor,
}

impl Default for ColorMode {
    fn default() -> Self {
        ColorMode::TextureColors
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Obj2Brs {
    pub bricktype: BrickType,
    pub brick_scale: isize,
    #[serde(skip)]
    input_file_path_receiver: Option<Receiver<Option<PathBuf>>>,
    input_file_path: String,
    pub match_brickadia_colorset: bool,
    #[serde(default)]
    color_mode: ColorMode,
    material: Material,
    material_intensity: u32,
    #[serde(skip)]
    output_directory_receiver: Option<Receiver<Option<PathBuf>>>,
    output_directory: String,
    save_owner_id: String,
    save_owner_name: String,
    save_name: String,
    scale: f32,
    simplify: bool,
    split_by_material: bool,
    grid_offset_x: f32,
    grid_offset_y: f32,
    grid_offset_z: f32,
    /// Rotation around X axis in 90° increments (0, 90, 180, 270)
    #[serde(default)]
    rotation_x: i32,
    /// Rotation around Y axis in 90° increments (0, 90, 180, 270)
    #[serde(default)]
    rotation_y: i32,
    /// Rotation around Z axis in 90° increments (0, 90, 180, 270)
    #[serde(default)]
    rotation_z: i32,
    /// Add origin marker with color-coded X/Y/Z axis bricks
    #[serde(default)]
    show_origin_marker: bool,
    /// Scale multiplier for X axis (default 1.0)
    #[serde(default = "default_axis_scale")]
    scale_x: f32,
    /// Scale multiplier for Y axis (default 1.0)
    #[serde(default = "default_axis_scale")]
    scale_y: f32,
    /// Scale multiplier for Z axis (default 1.0) - adjust to fix squished height
    #[serde(default = "default_axis_scale")]
    scale_z: f32,
    /// Use texture-based material mapping for BSP conversions
    #[serde(default)]
    use_material_mapping: bool,
    /// Group output grids by Brickadia material type (Plastic, Glass, Glow, etc.)
    /// instead of one grid per texture. Can reduce 300+ grids to ~6 grids in some instances.
    #[serde(default)]
    group_by_brick_material: bool,
    /// Use smooth bricks without studs for a polished finish
    #[serde(default)]
    use_smooth_bricks: bool,
    /// Color merge threshold for combining similar colored blocks (0.0 = off, 0.1-1.0 = sensitivity)
    #[serde(default)]
    color_merge_threshold: f32,
    #[serde(skip)]
    missing_resources_dialog: Option<String>,
    #[serde(skip)]
    pending_conversion_skip_textures: bool,
    #[serde(skip)]
    logger: Logger,
    #[serde(skip)]
    conversion_in_progress: bool,
    #[serde(skip)]
    conversion_done_receiver: Option<Receiver<()>>,
    #[serde(skip)]
    conversion_progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    #[serde(skip)]
    conversion_stage: std::sync::Arc<std::sync::Mutex<String>>,
    #[serde(skip)]
    conversion_cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[serde(skip)]
    conversion_start_time: Option<std::time::Instant>,

    // BSP conversion options
    /// Detected or selected input file type.
    #[serde(default)]
    input_file_type: InputFileType,
    /// Selected game source for BSP conversion.
    #[serde(default)]
    bsp_game_source: GameSource,
    /// Detected game source (for display, may differ from selected).
    #[serde(skip)]
    detected_game_source: Option<GameSource>,
    
    // Settings profiles
    /// Saved settings profiles
    #[serde(default)]
    profiles: Vec<SettingsProfile>,
    /// Currently selected profile index (None = custom/unsaved)
    #[serde(skip)]
    selected_profile_index: Option<usize>,
    /// Name for new profile when saving
    #[serde(skip)]
    new_profile_name: String,
}

fn default_axis_scale() -> f32 {
    1.0
}

/// A settings profile containing conversion settings (not paths).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsProfile {
    pub name: String,
    pub bricktype: BrickType,
    pub brick_scale: isize,
    pub material: Material,
    pub material_intensity: u32,
    pub scale: f32,
    pub simplify: bool,
    pub match_brickadia_colorset: bool,
    #[serde(default)]
    pub color_mode: ColorMode,
    pub rotation_x: i32,
    pub rotation_y: i32,
    pub rotation_z: i32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_z: f32,
    pub show_origin_marker: bool,
}

impl Default for SettingsProfile {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            bricktype: BrickType::Microbricks,
            brick_scale: 1,
            material: Material::Plastic,
            material_intensity: 5,
            scale: 1.0,
            simplify: false,
            match_brickadia_colorset: false,
            color_mode: ColorMode::TextureColors,
            rotation_x: 0,
            rotation_y: 0,
            rotation_z: 0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            show_origin_marker: false,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum BrickType {
    Microbricks,
    Default,
    Tiles,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum Material {
    Plastic,
    Glass,
    Glow,
    Metallic,
    Hologram,
    Ghost,
}

impl Default for Obj2Brs {
    fn default() -> Self {
        // Get auto-suggested paths from data directory
        let imports_dir = logger::get_imports_dir();
        let exports_dir = logger::get_exports_dir();

        // Use data/imports as default input path hint, data/exports as output
        let default_input = if imports_dir.exists() {
            imports_dir.to_string_lossy().to_string()
        } else {
            "".into()
        };

        let default_output = exports_dir.to_string_lossy().to_string();

        Self {
            bricktype: BrickType::Microbricks,
            brick_scale: 1,
            input_file_path_receiver: None,
            input_file_path: default_input,
            match_brickadia_colorset: false,
            color_mode: ColorMode::TextureColors,
            material: Material::Plastic,
            material_intensity: 5,
            output_directory_receiver: None,
            output_directory: default_output,
            save_owner_id: "d66c4ad5-59fc-4a9b-80b8-08dedc25bff9".into(),
            save_owner_name: "obj2brz".into(),
            save_name: "converted".into(),
            scale: 1.0,
            simplify: false,
            split_by_material: false,
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_offset_z: 0.0,
            rotation_x: 0,
            rotation_y: 0,
            rotation_z: 0,
            show_origin_marker: false,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            use_material_mapping: false,
            group_by_brick_material: false,
            use_smooth_bricks: false,
            color_merge_threshold: 0.0,
            missing_resources_dialog: None,
            pending_conversion_skip_textures: false,
            logger: Logger::default(),
            conversion_in_progress: false,
            conversion_done_receiver: None,
            conversion_progress: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            conversion_stage: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
            conversion_cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            conversion_start_time: None,
            // BSP options
            input_file_type: InputFileType::Obj,
            bsp_game_source: GameSource::Auto,
            detected_game_source: None,
            // Profiles
            profiles: Vec::new(),
            selected_profile_index: None,
            new_profile_name: String::new(),
        }
    }
}

impl App for Obj2Brs {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint to keep updating logs
        ctx.request_repaint();

        self.receive_file_dialog_messages();

        let input_file_valid = Path::new(&self.input_file_path).exists();
        let output_dir_valid = Path::new(&self.output_directory).is_dir();
        let uuid_valid = Uuid::parse_str(&self.save_owner_id).is_ok();
        let can_convert = input_file_valid && output_dir_valid && uuid_valid && !self.conversion_in_progress;

        // Show missing resources dialog if needed
        self.show_missing_resources_dialog(ctx);

        // Footer at very bottom (must be created first to be at bottom)
        gui::footer(ctx);

        // Log panel above footer - resizable
        TopBottomPanel::bottom("log_panel")
            .resizable(true)
            .min_height(100.0)
            .default_height(ctx.screen_rect().height() / 4.0)
            .show(ctx, |ui| {
                // Voxelize button (centered above progress bar)
                ui.add_space(5.);
                ui.horizontal(|ui| {
                    let available_width = ui.available_width();
                    ui.add_space((available_width - 60.0) / 2.0);
                    if gui::button(ui, "Voxelize", can_convert && !self.conversion_in_progress) {
                        self.do_conversion()
                    }
                });
                ui.add_space(5.);
                
                // Progress bar (shown during conversion)
                if self.conversion_in_progress {
                    let progress = self.conversion_progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0;
                    let stage = self.conversion_stage.lock().map(|s| s.clone()).unwrap_or_default();
                    
                    // Calculate elapsed time
                    let elapsed_text = if let Some(start_time) = self.conversion_start_time {
                        let elapsed = start_time.elapsed();
                        let seconds = elapsed.as_secs();
                        if seconds < 60 {
                            format!("{}s", seconds)
                        } else {
                            let minutes = seconds / 60;
                            let secs = seconds % 60;
                            format!("{}m {}s", minutes, secs)
                        }
                    } else {
                        "0s".to_string()
                    };
                    
                    // Status bar with Converting label, elapsed time, and Cancel button
                    ui.horizontal(|ui| {
                        ui.add_enabled(false, egui::Button::new("Converting..."));
                        ui.label(RichText::new(elapsed_text).color(egui::Color32::GRAY).monospace());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                self.conversion_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                                self.logger.log("Cancellation requested...".to_string());
                            }
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.add(egui::ProgressBar::new(progress)
                            .show_percentage()
                            .animate(true));
                    });
                    if !stage.is_empty() {
                        ui.label(RichText::new(&stage).color(egui::Color32::YELLOW));
                    }
                    ui.add_space(2.);
                }

                Frame::default()
                    .fill(egui::Color32::from_gray(20))
                    .inner_margin(egui::Margin { left: 8.0, right: 16.0, top: 8.0, bottom: 8.0 })
                    .show(ui, |ui| {
                        ui.set_min_height(ui.available_height());
                        ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .auto_shrink([false, false])
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                            .show(ui, |ui| {
                                let messages = self.logger.get_messages();
                                if messages.is_empty() {
                                    ui.label(
                                        RichText::new("No logs yet...")
                                            .color(egui::Color32::GRAY)
                                            .monospace()
                                    );
                                } else {
                                    for message in messages {
                                        ui.label(
                                            RichText::new(message)
                                                .color(egui::Color32::LIGHT_GREEN)
                                                .monospace()
                                        );
                                    }
                                }
                            });
                    });
            });

        // Main content area
        CentralPanel::default().show(ctx, |ui: &mut Ui| {
            ScrollArea::vertical().show(ui, |ui| {
                gui::add_grid(ui, "paths_grid", |ui| self.paths(ui, input_file_valid, output_dir_valid));
                gui::add_horizontal_line(ui);
                
                // Settings profiles section
                ui.add_space(5.);
                self.profiles_ui(ui);
                ui.add_space(5.);
                gui::add_horizontal_line(ui);
                
                gui::add_grid(ui, "options_grid", |ui| self.options(ui, uuid_valid));

                ui.add_space(5.);
                CollapsingHeader::new("Advanced Options")
                    .default_open(false)
                    .show(ui, |ui| {
                        gui::add_grid(ui, "advanced_options_grid", |ui| self.advanced_options(ui, uuid_valid));
                        ui.add_space(5.);
                        gui::info_text(ui);
                    });

                ui.add_space(5.);
                CollapsingHeader::new("Cache & Data")
                    .default_open(false)
                    .show(ui, |ui| {
                        self.cache_options(ui);
                    });

                ui.add_space(10.);
            });

        });
    }
}

impl Obj2Brs {
    /// Update detected file type and game source based on input path.
    fn update_input_file_type(&mut self) {
        let path = Path::new(&self.input_file_path);

        if is_bsp_file(path) {
            self.input_file_type = InputFileType::Bsp;

            // Try to detect game source
            if path.exists() {
                match detect_game_source(path) {
                    Ok(game) => {
                        self.detected_game_source = Some(game);
                        // Auto-select if user hasn't chosen
                        if self.bsp_game_source == GameSource::Auto {
                            self.bsp_game_source = game;
                        }
                    }
                    Err(_) => {
                        self.detected_game_source = None;
                    }
                }
            } else {
                self.detected_game_source = None;
            }
        } else {
            self.input_file_type = InputFileType::Obj;
            self.detected_game_source = None;
        }
    }

    fn receive_file_dialog_messages(&mut self) {
        if let Some(rx) = &self.input_file_path_receiver {
            if let Ok(data) = rx.try_recv() {
                self.input_file_path_receiver = None;
                if let Some(path) = data {
                    if let Ok(path_str) = path.clone().into_os_string().into_string() {
                        self.input_file_path = path_str;
                        self.update_input_file_type();
                    }
                }
            }
        }

        if let Some(rx) = &self.output_directory_receiver {
            if let Ok(data) = rx.try_recv() {
                self.output_directory_receiver = None;
                if let Some(path) = data {
                    if let Ok(path_str) = path.into_os_string().into_string() {
                        self.output_directory = path_str;
                    }
                }
            }
        }

        // Check if conversion is done
        if let Some(rx) = &self.conversion_done_receiver {
            if rx.try_recv().is_ok() {
                self.conversion_done_receiver = None;
                self.conversion_in_progress = false;
                self.conversion_start_time = None;
            }
        }
    }

    fn paths(&mut self, ui: &mut Ui, input_file_valid: bool, output_dir_valid: bool) {
        let file_color = gui::bool_color(input_file_valid);

        // Dynamic label based on detected file type
        let file_label = match self.input_file_type {
            InputFileType::Obj => "OBJ File",
            InputFileType::Bsp => "BSP File",
        };
        let file_tooltip = match self.input_file_type {
            InputFileType::Obj => "Wavefront OBJ model to convert",
            InputFileType::Bsp => "BSP map file (will be converted to OBJ first)",
        };

        ui.label(file_label).on_hover_text(file_tooltip);
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.input_file_path)
                    .desired_width(400.0)
                    .text_color(file_color),
            );
            if gui::file_button(ui) && self.input_file_path_receiver.is_none() {
                let (tx, rx) = mpsc::channel();
                self.input_file_path_receiver = Some(rx);
                thread::spawn(move || {
                    let file_path = FileDialog::new()
                        .add_filter("3D Models", &["obj", "bsp"])
                        .add_filter("OBJ", &["obj"])
                        .add_filter("BSP", &["bsp"])
                        .pick_file();
                    let _ = tx.send(file_path);
                });
            }
        });
        ui.end_row();

        // Show BSP-specific options when a BSP file is selected
        if self.input_file_type == InputFileType::Bsp {
            ui.label("Game Source").on_hover_text(
                "Select the game this BSP file is from (auto-detected if possible)"
            );
            ui.horizontal(|ui| {
                ComboBox::from_id_source("bsp_game_source")
                    .selected_text(self.bsp_game_source.display_name())
                    .show_ui(ui, |ui: &mut Ui| {
                        ui.selectable_value(
                            &mut self.bsp_game_source,
                            GameSource::Auto,
                            "Auto-detect"
                        );
                        ui.separator();
                        for game in GameSource::all() {
                            let label = if game.is_implemented() {
                                game.display_name().to_string()
                            } else {
                                format!("{} (not implemented)", game.display_name())
                            };
                            ui.selectable_value(&mut self.bsp_game_source, *game, label);
                        }
                    });

                // Show detected game source
                if let Some(detected) = self.detected_game_source {
                    ui.label(
                        RichText::new(format!("(Detected: {})", detected.display_name()))
                            .color(egui::Color32::LIGHT_GREEN)
                            .small()
                    );
                }
            });
            ui.end_row();
            
            // Show texture assets status with green/yellow/red indicator
            let game_source = self.detected_game_source.unwrap_or(self.bsp_game_source);
            if game_source != GameSource::Auto {
                let (exists, has_files, path_opt) = material_mapping::MaterialMapping::check_assets_dir(game_source);
                let has_config = material_mapping::MaterialMapping::find_config_path(game_source).is_some();
                
                ui.label("Texture Assets").on_hover_text(
                    "Status of extracted game textures for this game source.\n\n\
                    Place your legally extracted game textures in the assets/ folder\n\
                    to enable texture-based color sampling during conversion."
                );
                
                ui.horizontal(|ui| {
                    // Config status
                    let config_icon = if has_config { "C" } else { "C" };
                    let config_color = if has_config { egui::Color32::LIGHT_GREEN } else { egui::Color32::YELLOW };
                    let config_tooltip = if has_config {
                        "Material config found (auto_2block_materials.yaml)"
                    } else {
                        "No material config found - create auto_2block_materials.yaml"
                    };
                    ui.label(RichText::new(config_icon).color(config_color).strong())
                        .on_hover_text(config_tooltip);
                    
                    // Assets status
                    let (assets_icon, assets_color, assets_tooltip) = if has_files {
                        ("A", egui::Color32::LIGHT_GREEN, "Assets directory has texture files")
                    } else if exists {
                        ("A", egui::Color32::YELLOW, "Assets directory exists but is empty - add extracted textures")
                    } else {
                        ("A", egui::Color32::from_rgb(255, 100, 100), "Assets directory not found")
                    };
                    ui.label(RichText::new(assets_icon).color(assets_color).strong())
                        .on_hover_text(assets_tooltip);
                    
                    // Show path info
                    if let Some(path) = path_opt {
                        let status_text = if has_files {
                            "Ready"
                        } else if exists {
                            "Empty"
                        } else {
                            "Missing"
                        };
                        ui.label(RichText::new(format!("[{}]", status_text)).small().color(assets_color));
                        
                        // Show path on hover
                        ui.label(RichText::new("(?)").small().color(egui::Color32::GRAY))
                            .on_hover_text(format!("Assets path: {}", path.display()));
                    }
                });
                ui.end_row();
            }
        }

        let dir_color = gui::bool_color(output_dir_valid);

        ui.label("Output Directory")
            .on_hover_text("Where generated save will be written to");
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.output_directory)
                    .desired_width(360.0)
                    .text_color(dir_color),
            );
            if gui::file_button(ui) && self.output_directory_receiver.is_none() {
                let (tx, rx) = mpsc::channel();
                self.output_directory_receiver = Some(rx);
                let default_dir = self.output_directory.clone();
                thread::spawn(move || {
                    let mut dialog = FileDialog::new();
                    if output_dir_valid {
                        dialog = dialog.set_directory(Path::new(default_dir.as_str()));
                    }
                    let output_dir = dialog.pick_folder();
                    let _ = tx.send(output_dir);
                });
            }
            // Button to open output folder in file explorer
            if output_dir_valid {
                if ui.button("Open").on_hover_text("Open output folder in file explorer").clicked() {
                    let output_path = self.output_directory.clone();
                    thread::spawn(move || {
                        let _ = open_folder_in_explorer(&output_path);
                    });
                }
            }
        });
        ui.end_row();

        ui.label("Save Name")
            .on_hover_text("Name for the Brickadia savefile (.brz format)");
        ui.add(TextEdit::singleline(&mut self.save_name));
        ui.end_row();
    }

    fn options(&mut self, ui: &mut Ui, _uuid_valid: bool) {
        ui.label("Lossy Conversion").on_hover_text(
            "Merges adjacent bricks of similar colors to reduce brick count.\n\n\
            This significantly reduces file size but may lose fine detail.\n\
            Recommended for large models.\n\n\
            WARNING: This is computationally expensive and can take 5-10+ minutes for complex models.",
        );
        ui.horizontal(|ui| {
            ui.add(Checkbox::new(&mut self.simplify, "Simplify (reduces brickcount)"));
            ui.label(RichText::new("(Expensive)").small().color(egui::Color32::from_rgb(255, 180, 0)));
        });
        ui.end_row();

        ui.label("Brick Surface").on_hover_text(
            "Choose between studded or smooth brick surfaces.\n\n\
            Studded: Traditional LEGO-style bricks with visible studs on top.\n\
            Smooth: Flat surfaces without studs for a polished, clean finish.\n\n\
            Smooth bricks are ideal for architectural models and clean surfaces.");
        ui.add(Checkbox::new(&mut self.use_smooth_bricks, "Use smooth bricks (no studs)"));
        ui.end_row();

        ui.label("Color Merge").on_hover_text(
            "Merge adjacent bricks with similar colors to reduce brick count.\n\n\
            Uses Delta E color difference for perceptual accuracy:\n\
            • Off: No merging\n\
            • Noticeable (1-2): Perceptible under close scrutiny\n\
            • Obvious (2-10): Visible at a glance\n\
            • Distinct (11-49): Clearly different, still similar\n\
            • Very Different (50-70): Major hue/lightness shifts\n\n\
            Only merges touching bricks within the same material group.");
        
        let current_label = if self.color_merge_threshold == 0.0 {
            "Off"
        } else if self.color_merge_threshold <= 2.0 {
            "Noticeable (1-2)"
        } else if self.color_merge_threshold <= 10.0 {
            "Obvious (2-10)"
        } else if self.color_merge_threshold <= 49.0 {
            "Distinct (11-49)"
        } else {
            "Very Different (50-70)"
        };
        
        egui::ComboBox::from_id_source("color_merge_combo")
            .selected_text(current_label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.color_merge_threshold, 0.0, "Off");
                ui.selectable_value(&mut self.color_merge_threshold, 1.5, "Noticeable (1-2)");
                ui.selectable_value(&mut self.color_merge_threshold, 6.0, "Obvious (2-10)");
                ui.selectable_value(&mut self.color_merge_threshold, 30.0, "Distinct (11-49)");
                ui.selectable_value(&mut self.color_merge_threshold, 60.0, "Very Different (50-70)");
            });
        ui.end_row();

        ui.label("Scale")
            .on_hover_text("Multiplier for the final build size in Brickadia.\n\n\
            x1.0 = 1 unit in OBJ equals 1 Brickadia unit.\n\
            x2.0 = Build will be twice as large.\n\
            x0.5 = Build will be half the size.\n\
            x0.125 = 1/8 scale (very small).\n\n\
            Range: 0.01 to 100.0");
        ui.add(
            DragValue::new(&mut self.scale)
                .min_decimals(2)
                .prefix("x")
                .speed(0.01)
                .range(0.01..=100.0),
        );
        ui.end_row();

        ui.label("Bricktype")
            .on_hover_text("The brick type used to build the model:\n\n\
            • Microbricks: Smallest bricks (2x2x2 studs). Best detail.\n\
            • Default: Standard bricks with visible studs.\n\
            • Tiles: Flat smooth bricks without studs.");
        ComboBox::from_id_source("bricktype_combo")
            .selected_text(format!("{:?}", &mut self.bricktype))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.bricktype, BrickType::Microbricks, "Microbricks");
                ui.selectable_value(&mut self.bricktype, BrickType::Default, "Default");
                ui.selectable_value(&mut self.bricktype, BrickType::Tiles, "Tiles");
            });
        ui.end_row();

        ui.label("Color Mode").on_hover_text("How colors are applied to bricks:\n\n\
            • Texture Colors: Use colors from model textures (default, most accurate).\n\
            • Material Color: Use the OBJ material's diffuse color.\n\
            • Single Color: Use one solid color for all bricks (based on Material setting below).");
        ComboBox::from_id_source("color_mode_combo")
            .selected_text(match self.color_mode {
                ColorMode::TextureColors => "Texture Colors",
                ColorMode::MaterialColor => "Material Color",
                ColorMode::SingleColor => "Single Color",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.color_mode, ColorMode::TextureColors, "Texture Colors");
                ui.selectable_value(&mut self.color_mode, ColorMode::MaterialColor, "Material Color");
                ui.selectable_value(&mut self.color_mode, ColorMode::SingleColor, "Single Color");
            });
        ui.end_row();

        ui.label("Material").on_hover_text("The Brickadia material applied to all bricks:\n\n\
            • Plastic: Standard opaque material.\n\
            • Glass: Transparent, see-through.\n\
            • Glow: Emits light.\n\
            • Metallic: Shiny reflective surface.\n\
            • Hologram: Translucent with glow effect.\n\
            • Ghost: Semi-transparent, no collision.");
        ComboBox::from_label("\n")
            .selected_text(format!("{:?}", &mut self.material))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.material, Material::Plastic, "Plastic");
                ui.selectable_value(&mut self.material, Material::Glass, "Glass");
                ui.selectable_value(&mut self.material, Material::Glow, "Glow");
                ui.selectable_value(&mut self.material, Material::Metallic, "Metallic");
                ui.selectable_value(&mut self.material, Material::Hologram, "Hologram");
                ui.selectable_value(&mut self.material, Material::Ghost, "Ghost");
            });
        ui.end_row();

        ui.label("");
        ui.vertical(|ui| {
            CollapsingHeader::new("Rotation & Orientation")
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(5.);
                    ui.label(RichText::new("Fix models that import facing the wrong direction or tilted")
                        .color(egui::Color32::GRAY));
                    ui.add_space(10.);
                    
                    gui::add_grid(ui, "rotation_grid", |ui| {
                        ui.label("Rotation X").on_hover_text(
                            "Rotate model around the X axis (left-right axis).\n\n\
                            In Brickadia coordinates:\n\
                            • 0° = No rotation\n\
                            • 90° = Tilt forward (top faces you)\n\
                            • 180° = Flip upside down\n\
                            • 270° = Tilt backward (bottom faces you)\n\n\
                            Use this to fix models that appear tilted forward/back.",
                        );
                        ComboBox::from_id_source("rotation_x")
                            .selected_text(format!("{}°", self.rotation_x))
                            .show_ui(ui, |ui: &mut Ui| {
                                ui.selectable_value(&mut self.rotation_x, 0, "0°");
                                ui.selectable_value(&mut self.rotation_x, 90, "90°");
                                ui.selectable_value(&mut self.rotation_x, 180, "180°");
                                ui.selectable_value(&mut self.rotation_x, 270, "270°");
                            });
                        ui.end_row();

                        ui.label("Rotation Y").on_hover_text(
                            "Rotate model around the Y axis (forward-back axis).\n\n\
                            In Brickadia coordinates:\n\
                            • 0° = No rotation\n\
                            • 90° = Roll left (left side up)\n\
                            • 180° = Flip left-right\n\
                            • 270° = Roll right (right side up)\n\n\
                            Use this to fix models that appear rolled/tilted sideways.",
                        );
                        ComboBox::from_id_source("rotation_y")
                            .selected_text(format!("{}°", self.rotation_y))
                            .show_ui(ui, |ui: &mut Ui| {
                                ui.selectable_value(&mut self.rotation_y, 0, "0°");
                                ui.selectable_value(&mut self.rotation_y, 90, "90°");
                                ui.selectable_value(&mut self.rotation_y, 180, "180°");
                                ui.selectable_value(&mut self.rotation_y, 270, "270°");
                            });
                        ui.end_row();

                        ui.label("Rotation Z").on_hover_text(
                            "Rotate model around the Z axis (up-down axis).\n\n\
                            In Brickadia coordinates:\n\
                            • 0° = No rotation\n\
                            • 90° = Spin 90° counter-clockwise (viewed from above)\n\
                            • 180° = Face opposite direction\n\
                            • 270° = Spin 90° clockwise (viewed from above)\n\n\
                            Use this to change which direction the model faces.",
                        );
                        ComboBox::from_id_source("rotation_z")
                            .selected_text(format!("{}°", self.rotation_z))
                            .show_ui(ui, |ui: &mut Ui| {
                                ui.selectable_value(&mut self.rotation_z, 0, "0°");
                                ui.selectable_value(&mut self.rotation_z, 90, "90°");
                                ui.selectable_value(&mut self.rotation_z, 180, "180°");
                                ui.selectable_value(&mut self.rotation_z, 270, "270°");
                            });
                        ui.end_row();

                        ui.label("Origin Marker").on_hover_text(
                            "Add color-coded axis markers at the origin (0,0,0) for alignment testing.\n\n\
                            In Brickadia coordinates:\n\
                            • White brick at origin center\n\
                            • Red bricks along +X axis (right in Brickadia)\n\
                            • Green bricks along +Y axis (forward in Brickadia)\n\
                            • Blue bricks along +Z axis (up in Brickadia)\n\n\
                            Useful for verifying model orientation after import.",
                        );
                        ui.add(Checkbox::new(&mut self.show_origin_marker, "Show XYZ Axis"));
                        ui.end_row();
                    });
                });
        });
        ui.end_row();

        ui.label("");
        ui.vertical(|ui| {
            CollapsingHeader::new("Per-Axis Scaling")
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(5.);
                    ui.label(RichText::new("Fix models that appear stretched or squished on specific axes")
                        .color(egui::Color32::GRAY));
                    ui.add_space(10.);
                    
                    gui::add_grid(ui, "axis_scale_grid", |ui| {
                        ui.label("Scale X").on_hover_text(
                            "Scale multiplier for the X axis (left-right in Brickadia).\n\n\
                            Default: 1.0 (no change)\n\
                            Use values > 1.0 to stretch, < 1.0 to compress.\n\n\
                            Adjust if model appears stretched or squished horizontally.",
                        );
                        ui.add(
                            DragValue::new(&mut self.scale_x)
                                .min_decimals(2)
                                .prefix("x")
                                .speed(0.01)
                                .range(0.1..=10.0),
                        );
                        ui.end_row();

                        ui.label("Scale Y").on_hover_text(
                            "Scale multiplier for the Y axis (forward-back in Brickadia).\n\n\
                            Default: 1.0 (no change)\n\
                            Use values > 1.0 to stretch, < 1.0 to compress.\n\n\
                            Adjust if model appears stretched or squished in depth.",
                        );
                        ui.add(
                            DragValue::new(&mut self.scale_y)
                                .min_decimals(2)
                                .prefix("x")
                                .speed(0.01)
                                .range(0.1..=10.0),
                        );
                        ui.end_row();

                        ui.label("Scale Z").on_hover_text(
                            "Scale multiplier for the Z axis (up-down in Brickadia).\n\n\
                            Default: 1.0 (no change)\n\
                            Use values > 1.0 to stretch vertically, < 1.0 to compress.\n\n\
                            **Common fix**: If model appears squished/flat, try 2.5 to compensate\n\
                            for Brickadia's plate height ratio (plates are 2.5x shorter than wide).",
                        );
                        ui.add(
                            DragValue::new(&mut self.scale_z)
                                .min_decimals(2)
                                .prefix("x")
                                .speed(0.01)
                                .range(0.1..=10.0),
                        );
                        ui.end_row();
                    });
                });
        });
        ui.end_row();
    }

    fn profiles_ui(&mut self, ui: &mut Ui) {
        let mut profile_to_load: Option<usize> = None;
        
        ui.horizontal(|ui| {
            ui.label("Profile:");
            
            // Profile dropdown
            let current_name = if let Some(idx) = self.selected_profile_index {
                if idx < self.profiles.len() {
                    self.profiles[idx].name.clone()
                } else {
                    "Custom".to_string()
                }
            } else {
                "Custom".to_string()
            };
            
            ComboBox::from_id_source("profile_select")
                .selected_text(&current_name)
                .show_ui(ui, |ui: &mut Ui| {
                    if ui.selectable_label(self.selected_profile_index.is_none(), "Custom").clicked() {
                        self.selected_profile_index = None;
                    }
                    for i in 0..self.profiles.len() {
                        let name = self.profiles[i].name.clone();
                        if ui.selectable_label(self.selected_profile_index == Some(i), &name).clicked() {
                            profile_to_load = Some(i);
                        }
                    }
                });
        });
        
        // Load profile outside the borrow
        if let Some(idx) = profile_to_load {
            self.load_profile(idx);
        }
        
        ui.horizontal(|ui| {
            // Save current settings as new profile
            ui.add(TextEdit::singleline(&mut self.new_profile_name).hint_text("Profile name").desired_width(120.0));
            if ui.button("Save").on_hover_text("Save current settings as a new profile").clicked() {
                if !self.new_profile_name.trim().is_empty() {
                    self.save_as_profile(self.new_profile_name.trim().to_string());
                    self.new_profile_name.clear();
                }
            }
            
            // Delete selected profile
            if self.selected_profile_index.is_some() {
                if ui.button("Delete").on_hover_text("Delete the selected profile").clicked() {
                    if let Some(idx) = self.selected_profile_index {
                        if idx < self.profiles.len() {
                            self.profiles.remove(idx);
                            self.selected_profile_index = None;
                        }
                    }
                }
            }
            
            // Reset to defaults button
            if ui.button("Reset").on_hover_text("Reset all settings to default values").clicked() {
                self.reset_to_defaults();
            }
        });
    }

    fn reset_to_defaults(&mut self) {
        let defaults = SettingsProfile::default();
        self.bricktype = defaults.bricktype;
        self.brick_scale = defaults.brick_scale;
        self.material = defaults.material;
        self.material_intensity = defaults.material_intensity;
        self.scale = defaults.scale;
        self.simplify = defaults.simplify;
        self.match_brickadia_colorset = defaults.match_brickadia_colorset;
        self.color_mode = defaults.color_mode;
        self.rotation_x = defaults.rotation_x;
        self.rotation_y = defaults.rotation_y;
        self.rotation_z = defaults.rotation_z;
        self.scale_x = defaults.scale_x;
        self.scale_y = defaults.scale_y;
        self.scale_z = defaults.scale_z;
        self.show_origin_marker = defaults.show_origin_marker;
        self.selected_profile_index = None;
        self.logger.log("Reset settings to defaults".to_string());
    }

    fn save_as_profile(&mut self, name: String) {
        let profile = SettingsProfile {
            name: name.clone(),
            bricktype: self.bricktype,
            brick_scale: self.brick_scale,
            material: self.material,
            material_intensity: self.material_intensity,
            scale: self.scale,
            simplify: self.simplify,
            match_brickadia_colorset: self.match_brickadia_colorset,
            color_mode: self.color_mode,
            rotation_x: self.rotation_x,
            rotation_y: self.rotation_y,
            rotation_z: self.rotation_z,
            scale_x: self.scale_x,
            scale_y: self.scale_y,
            scale_z: self.scale_z,
            show_origin_marker: self.show_origin_marker,
        };
        
        // Check if profile with same name exists, update it
        if let Some(idx) = self.profiles.iter().position(|p| p.name == name) {
            self.profiles[idx] = profile;
            self.selected_profile_index = Some(idx);
        } else {
            self.profiles.push(profile);
            self.selected_profile_index = Some(self.profiles.len() - 1);
        }
        
        self.logger.log(format!("Saved profile: {}", name));
    }

    fn load_profile(&mut self, index: usize) {
        if index >= self.profiles.len() {
            return;
        }
        
        let profile = &self.profiles[index];
        self.bricktype = profile.bricktype;
        self.brick_scale = profile.brick_scale;
        self.material = profile.material;
        self.material_intensity = profile.material_intensity;
        self.scale = profile.scale;
        self.simplify = profile.simplify;
        self.match_brickadia_colorset = profile.match_brickadia_colorset;
        self.color_mode = profile.color_mode;
        self.rotation_x = profile.rotation_x;
        self.rotation_y = profile.rotation_y;
        self.rotation_z = profile.rotation_z;
        self.scale_x = profile.scale_x;
        self.scale_y = profile.scale_y;
        self.scale_z = profile.scale_z;
        self.show_origin_marker = profile.show_origin_marker;
        
        self.selected_profile_index = Some(index);
        self.logger.log(format!("Loaded profile: {}", profile.name));
    }

    fn advanced_options(&mut self, ui: &mut Ui, uuid_valid: bool) {
        ui.label("Material Intensity").on_hover_text(
            "Controls the strength of the Brickadia material effect (0-10).\n\n\
            Higher values = stronger glow, more reflective metallic, etc.\n\
            Only affects Glass, Glow, Metallic, Hologram, and Ghost materials.",
        );
        ui.add(Slider::new(
            &mut self.material_intensity,
            RangeInclusive::new(0, 10),
        ));
        ui.end_row();

        ui.label("Match to Colorset").on_hover_text(
            "Snaps brick colors to Brickadia's default 64-color palette.\n\n\
            Enable this if you want colors that match standard Brickadia bricks.\n\
            Disable for more accurate color reproduction from the original model.",
        );
        ui.add(Checkbox::new(&mut self.match_brickadia_colorset, "Use Default Palette"));
        ui.end_row();

        ui.label("");
        ui.vertical(|ui| {
            CollapsingHeader::new("Experimental Material Processing")
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(5.);
                    
                    ui.label(RichText::new("WARNING: Processing large models with many materials can take 1+ hours")
                        .color(egui::Color32::from_rgb(255, 180, 0)));
                    
                    ui.add_space(10.);
                    
                    gui::add_grid(ui, "experimental_material_grid", |ui| {
                        ui.label("Split by Material").on_hover_text(
                            "Creates separate frozen brick grids for each OBJ material.\n\n\
                            Useful for models with distinct parts you want to move independently.\n\
                            Each material becomes its own selectable group in Brickadia.",
                        );
                        ui.add(Checkbox::new(&mut self.split_by_material, "Separate grids per material"));
                        ui.end_row();

                        // Material mapping option (requires split_by_material)
                        if self.split_by_material {
                            let game_source = self.detected_game_source.unwrap_or(self.bsp_game_source);
                            let has_config = game_source != GameSource::Auto && 
                                material_mapping::MaterialMapping::find_config_path(game_source).is_some();
                            
                            ui.label("Auto-assign Materials").on_hover_text(
                                "Map textures to Brickadia materials based on texture names.\n\n\
                                Uses YAML config files in data/game_textures/<engine>/<game>/auto_2block_materials.yaml\n\
                                to determine which textures should be Glass, Metallic, Glow, etc.\n\n\
                                Example: GLASS* textures = Glass material, METAL* = Metallic\n\n\
                                Each material can have its own intensity setting in the config.\n\n\
                                WARNING: This is computationally expensive for large models.\n\n\
                                When enabled, the global Material and Material Intensity settings are ignored.",
                            );
                            
                            ui.horizontal(|ui| {
                                if has_config {
                                    ui.add(Checkbox::new(&mut self.use_material_mapping, "Enable"));
                                    ui.label(RichText::new("(Expensive)").small().color(egui::Color32::from_rgb(255, 180, 0)));
                                } else {
                                    ui.add_enabled(false, Checkbox::new(&mut false, "Enable"));
                                    if game_source == GameSource::Auto {
                                        ui.label(RichText::new("(Select a game source)").small().color(egui::Color32::GRAY));
                                    } else {
                                        ui.label(RichText::new(format!("(No config for {})", game_source.display_name())).small().color(egui::Color32::GRAY));
                                    }
                                }
                            });
                            ui.end_row();
                            
                            // Show info about where to put config if missing
                            if !has_config && game_source != GameSource::Auto {
                                ui.label("");
                                ui.label(RichText::new(
                                    format!("Create: data/game_textures/{}/*/auto_2block_materials.yaml", game_source.folder_name())
                                ).small().color(egui::Color32::DARK_GRAY));
                                ui.end_row();
                            }
                            
                            // Group by Brickadia material type option
                            ui.label("Group by Material Type").on_hover_text(
                                "Consolidate output grids by Brickadia material type.\n\n\
                                OFF: One grid per texture (300+ grids for complex maps)\n\
                                ON: One grid per material type (~6 grids: Plastic, Glass, Glow, Metallic, etc.)\n\n\
                                Fewer grids = simpler to manage, but less granular selection in-game.",
                            );
                            ui.add(Checkbox::new(&mut self.group_by_brick_material, "Consolidate grids"));
                            ui.end_row();
                        }

                        if self.split_by_material {
                            ui.label("Grid Offset X").on_hover_text(
                                "Horizontal spacing between material grids",
                            );
                            ui.add(DragValue::new(&mut self.grid_offset_x).suffix(" units").speed(10.0));
                            ui.end_row();

                            ui.label("Grid Offset Y").on_hover_text(
                                "Forward/back spacing between material grids",
                            );
                            ui.add(DragValue::new(&mut self.grid_offset_y).suffix(" units").speed(10.0));
                            ui.end_row();

                            ui.label("Grid Offset Z").on_hover_text(
                                "Vertical spacing between material grids",
                            );
                            ui.add(DragValue::new(&mut self.grid_offset_z).suffix(" units").speed(10.0));
                            ui.end_row();
                        }
                    });
                });
        });
        ui.end_row();

        if self.bricktype == BrickType::Microbricks {
            ui.label("Brick Scale")
                .on_hover_text("Multiplies the size of each microbrick (1-500).\n\n\
            x1 = Standard microbrick size (highest detail).\n\
            x2 = Each voxel becomes 2x2x2 microbricks (blockier look).\n\
            Higher values = more pixelated/chunky appearance.\n\n\
            Note: This is different from 'Scale' which affects the overall build size.");
            ui.add(
                DragValue::new(&mut self.brick_scale)
                    .prefix("x")
                    .range(1..=500),
            );
            ui.end_row();
        }

        let id_color = bool_color(uuid_valid);

        ui.label("Brick Owner")
            .on_hover_text("Who will have ownership of the generated bricks");
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut self.save_owner_name).desired_width(100.0));
            ui.add(
                TextEdit::singleline(&mut self.save_owner_id)
                    .desired_width(300.0)
                    .text_color(id_color),
            );
        });
        ui.end_row();
    }

    fn cache_options(&mut self, ui: &mut Ui) {
        let cache_dir = logger::get_cache_dir();
        let cache_path_str = cache_dir.to_string_lossy().to_string();

        ui.horizontal(|ui| {
            ui.label("User Cache Location:");
            ui.add(TextEdit::singleline(&mut cache_path_str.clone())
                .desired_width(350.0)
                .interactive(false));
            if ui.button("Open").on_hover_text("Open cache folder").clicked() {
                let path = cache_path_str.clone();
                thread::spawn(move || {
                    let _ = open_folder_in_explorer(&path);
                });
            }
        });

        ui.add_space(5.);
        ui.horizontal(|ui| {
            ui.label("Cache contains: app settings, window size, logs");
        });

        ui.add_space(5.);
        ui.horizontal(|ui| {
            if ui.button("Clear Cache").on_hover_text("Delete all cached data (settings will reset on next launch)").clicked() {
                if let Err(e) = logger::flush_cache() {
                    self.logger.log(format!("Failed to clear cache: {}", e));
                } else {
                    self.logger.log("Cache cleared. Settings will reset on next launch.".to_string());
                }
            }
            ui.label(RichText::new("(Requires restart to take effect)").small().color(egui::Color32::GRAY));
        });

        ui.add_space(5.);
        let data_dir = logger::get_data_dir();
        ui.horizontal(|ui| {
            ui.label("Data Directory:");
            let data_path_str = data_dir.to_string_lossy().to_string();
            ui.add(TextEdit::singleline(&mut data_path_str.clone())
                .desired_width(350.0)
                .interactive(false));
            if ui.button("Open").on_hover_text("Open data folder").clicked() {
                let path = data_path_str.clone();
                thread::spawn(move || {
                    let _ = open_folder_in_explorer(&path);
                });
            }
        });

        ui.add_space(5.);
        ui.horizontal(|ui| {
            ui.label("Brickadia Prefabs:");
            let prefabs_path = match env::consts::OS {
                "windows" => {
                    dirs::data_local_dir()
                        .map(|p| p.join("Brickadia").join("Saved").join("Prefabs"))
                        .and_then(|p| p.to_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "Not found".to_string())
                }
                "linux" => {
                    dirs::config_dir()
                        .map(|p| p.join("Epic").join("Brickadia").join("Saved").join("Prefabs"))
                        .and_then(|p| p.to_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "Not found".to_string())
                }
                _ => "Not supported".to_string(),
            };
            ui.add(TextEdit::singleline(&mut prefabs_path.clone())
                .desired_width(350.0)
                .interactive(false));
            if ui.button("Open").on_hover_text("Open Brickadia Prefabs folder (paste .brz files here)").clicked() {
                let path = prefabs_path.clone();
                thread::spawn(move || {
                    let _ = open_folder_in_explorer(&path);
                });
            }
        });
    }

    fn show_missing_resources_dialog(&mut self, ctx: &egui::Context) {
        if let Some(message) = &self.missing_resources_dialog.clone() {
            let mut open = true;
            Window::new("Missing Resources")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .fixed_size([500.0, 400.0])
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                ui.label(message);
                            });

                        ui.add_space(10.);
                        ui.separator();
                        ui.add_space(10.);

                        ui.label("Do you want to continue without the missing textures?");
                        ui.add_space(5.);
                        ui.label("• Yes: Use solid colors from material definitions");
                        ui.label("• No: Cancel conversion so you can fix the missing textures");

                        ui.add_space(10.);
                        ui.horizontal(|ui| {
                            ui.add_space(100.0);
                            if ui.button("Yes").clicked() {
                                self.missing_resources_dialog = None;
                                self.pending_conversion_skip_textures = true;
                                self.continue_conversion(true);
                            }
                            if ui.button("No").clicked() {
                                self.missing_resources_dialog = None;
                                self.pending_conversion_skip_textures = false;
                            }
                        });
                    });
                });

            if !open {
                self.missing_resources_dialog = None;
                self.pending_conversion_skip_textures = false;
            }
        }
    }

    fn do_conversion(&mut self) {
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

    fn do_bsp_conversion(&mut self) {
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
            let temp_dir = if DEBUG_MODE {
                logger::get_exports_dir().join("bsp_temp")
            } else {
                std::env::temp_dir().join("obj2brz_bsp_temp")
            };
            if let Err(e) = std::fs::create_dir_all(&temp_dir) {
                logger.log(format!("Error creating temp directory: {}", e));
                let _ = tx.send(());
                return;
            }
            if DEBUG_MODE {
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

            let opts = Obj2Brs {
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

    fn continue_conversion(&mut self, skip_textures: bool) {
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
            // Create a minimal Obj2Brs for the conversion functions
            let opts = Obj2Brs {
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

/// Opens a folder in the system file explorer.
///
/// Cross-platform function that opens the specified folder path in:
/// - Windows: File Explorer
/// - macOS: Finder
/// - Linux: Default file manager
fn open_folder_in_explorer(path: &str) -> std::io::Result<()> {
    let path = Path::new(path);
    
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Path does not exist",
        ));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()?;
    }

    Ok(())
}

/// Creates a 1x1 solid color texture from material color
fn create_solid_color_texture(diffuse: [f32; 3], dissolve: f32) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(1, 1);
    img.put_pixel(
        0,
        0,
        image::Rgba([
            color::ftoi(diffuse[0]),
            color::ftoi(diffuse[1]),
            color::ftoi(diffuse[2]),
            color::ftoi(dissolve),
        ]),
    );
    img
}

/// Validates OBJ file and checks for missing resources
fn validate_obj_resources(obj_path: &str) -> ConversionResult<MissingResources> {
    let p = Path::new(obj_path);

    // Check if OBJ file exists
    if !p.exists() {
        return Err(ConversionError::ObjFileNotFound { path: p.to_path_buf() });
    }

    let load_options = LoadOptions {
        triangulate: true,
        ignore_lines: true,
        ignore_points: true,
        single_index: true,
    };

    let (_models, materials) = tobj::load_obj(obj_path, &load_options)
        .map_err(|e| ConversionError::ObjParseError(e.to_string()))?;

    let mut missing = MissingResources::new();

    // Check if materials exist
    let materials = match materials {
        Ok(mats) if !mats.is_empty() => mats,
        Ok(_) | Err(_) => {
            missing.missing_materials = true;
            return Ok(missing);
        }
    };

    // Check each material for missing textures
    for material in materials {
        if let Some(texture_name) = &material.diffuse_texture {
            if !texture_name.is_empty() {
                let texture_path = p.parent()
                    .ok_or_else(|| ConversionError::ObjFileNotFound { path: p.to_path_buf() })?
                    .join(texture_name);

                if !texture_path.exists() {
                    missing.missing_textures.push((material.name.clone(), texture_path));
                }
            }
        }
    }

    Ok(missing)
}

fn set_progress(opts: &Obj2Brs, percent: u32, stage: &str) {
    opts.conversion_progress.store(percent, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut s) = opts.conversion_stage.lock() {
        *s = stage.to_string();
    }
}

/// Check if the conversion has been cancelled
fn is_cancelled(opts: &Obj2Brs) -> bool {
    opts.conversion_cancelled.load(std::sync::atomic::Ordering::Relaxed)
}

/// Log a debug message only when DEBUG_MODE is enabled
fn debug_log(logger: &Logger, message: String) {
    if DEBUG_MODE {
        logger.log(format!("[DEBUG] {}", message));
    }
}

/// Format a number with comma separators for readability (e.g., 1234567 -> "1,234,567")
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn perform_conversion(opts: &Obj2Brs, skip_textures: bool) -> ConversionResult<()> {
    if is_cancelled(opts) {
        opts.logger.log("Conversion cancelled.".to_string());
        return Ok(());
    }
    
    // Log conversion settings for debugging
    opts.logger.log("=== CONVERSION SETTINGS ===".to_string());
    opts.logger.log(format!("Scale: {}", opts.scale));
    opts.logger.log(format!("Brick Scale: {}", opts.brick_scale));
    opts.logger.log(format!("Brick Type: {:?}", opts.bricktype));
    opts.logger.log(format!("Material: {:?} (Intensity: {})", opts.material, opts.material_intensity));
    opts.logger.log(format!("Color Mode: {:?}", opts.color_mode));
    opts.logger.log(format!("Match Brickadia Colorset: {}", opts.match_brickadia_colorset));
    opts.logger.log(format!("Simplify: {}", opts.simplify));
    opts.logger.log(format!("Use Smooth Bricks: {}", opts.use_smooth_bricks));
    opts.logger.log(format!("Color Merge Threshold: {}", opts.color_merge_threshold));
    opts.logger.log(format!("Axis Scale: X={}, Y={}, Z={}", opts.scale_x, opts.scale_y, opts.scale_z));
    opts.logger.log(format!("Rotation: X={}°, Y={}°, Z={}°", opts.rotation_x, opts.rotation_y, opts.rotation_z));
    opts.logger.log(format!("Show Origin Marker: {}", opts.show_origin_marker));
    opts.logger.log(format!("Skip Textures: {}", skip_textures));
    
    // Log experimental material processing settings
    opts.logger.log(format!("Split by Material: {}", opts.split_by_material));
    if opts.split_by_material {
        opts.logger.log(format!("  - Use Material Mapping: {}", opts.use_material_mapping));
        opts.logger.log(format!("  - Group by Brick Material: {}", opts.group_by_brick_material));
        opts.logger.log(format!("  - Grid Offset: X={}, Y={}, Z={}", opts.grid_offset_x, opts.grid_offset_y, opts.grid_offset_z));
    }
    opts.logger.log("===========================".to_string());
    opts.logger.log("".to_string());
    
    if opts.split_by_material {
        // Load models and materials once
        set_progress(opts, 5, "Loading models and materials...");
        opts.logger.log("Loading models and materials...".to_string());
        let (mut models, material_images, material_names) = load_models_and_materials(opts, skip_textures)?;
        let material_count = material_images.len();

        // Load material mapping with cache if enabled
        let mut cached_mapping = if opts.use_material_mapping {
            let game_source = opts.detected_game_source.unwrap_or(opts.bsp_game_source);
            if game_source != GameSource::Auto {
                match material_mapping::CachedMaterialMapping::load(game_source) {
                    Some(mut mapping) => {
                        let initial_cache_size = mapping.cache_size();
                        
                        // Pre-populate cache with all prefixes from config if cache is empty or small
                        if initial_cache_size < 10 {
                            opts.logger.log("Building material cache from config prefixes...".to_string());
                            mapping.build_cache_from_config();
                            let new_cache_size = mapping.cache_size();
                            if new_cache_size > initial_cache_size {
                                opts.logger.log(format!("Pre-populated cache with {} entries", new_cache_size - initial_cache_size));
                            }
                        }
                        
                        opts.logger.log(format!("Loaded material mapping for {} ({} cached entries)", game_source.display_name(), mapping.cache_size()));
                        Some(mapping)
                    }
                    None => {
                        opts.logger.log(format!("No material mapping found for {}, using default material", game_source.display_name()));
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        if material_count == 0 {
            opts.logger.log("No materials found, falling back to single grid".to_string());
            set_progress(opts, 20, "Voxelizing...");
            let mut octree = voxelize_models(&mut models, &material_images, opts, None);
            set_progress(opts, 70, "Writing BRZ file...");
            return write_brz_data(&mut octree, opts, None);
        }

        opts.logger.log(format!("Found {} materials, processing each separately", material_count));

        // Pre-extract and group triangles by material (Option A optimization)
        // This avoids re-parsing all models for each material
        set_progress(opts, 8, "Pre-grouping triangles by material...");
        opts.logger.log("Pre-grouping triangles by material...".to_string());
        let pregrouped = PreGroupedTriangles::from_models(&models);
        opts.logger.log(format!("Extracted {} triangles into {} material groups", 
            pregrouped.total_count, pregrouped.by_material.len()));
        
        // Debug: show which material IDs actually have triangles
        for (mat_key, tris) in &pregrouped.by_material {
            opts.logger.log(format!("  Material {:?}: {} triangles", mat_key, tris.len()));
        }

        // Pre-compute material mappings before parallel processing (avoids mutable borrow issues)
        let material_mappings: Vec<(Material, u8)> = if let Some(ref mut mapping) = cached_mapping {
            (0..material_count).map(|mat_id| {
                let mat_name = material_names.get(mat_id).cloned().unwrap_or_else(|| format!("material_{}", mat_id));
                mapping.get_material_and_intensity(&mat_name)
            }).collect()
        } else {
            vec![(opts.material, opts.material_intensity as u8); material_count]
        };

        // Process each material separately
        // Note: brdb types (Brick, Entity) are not Send/Sync, so we process sequentially
        // but benefit from Option A optimization (pre-grouped triangles)
        
        // If grouping by brick material type, collect bricks into buckets first
        use std::collections::HashMap;
        let mut bricks_by_type: HashMap<Material, Vec<Brick>> = HashMap::new();
        let mut material_grids: Vec<(Entity, Vec<Brick>)> = Vec::new();

        let mut skipped_count = 0;
        let mut processed_count = 0;
        let mut total_processing_time = 0.0f32;
        let mut avg_time_per_material = 1.0f32; // Start with 1 second assumption
        
        for mat_id in 0..material_count {
            if is_cancelled(opts) {
                opts.logger.log("Conversion cancelled.".to_string());
                return Ok(());
            }
            
            let mat_name = material_names.get(mat_id).cloned().unwrap_or_else(|| format!("material_{}", mat_id));
            
            // Early skip: check if material has triangles and reasonable bounds
            let triangles = pregrouped.get_material(mat_id);
            let bounds = pregrouped.get_material_bounds(mat_id);
            
            if triangles.is_none() || triangles.map(|t| t.is_empty()).unwrap_or(true) {
                skipped_count += 1;
                debug_log(&opts.logger, format!("Material {} ({}) - SKIPPED: no triangles found in pregrouped data", 
                    mat_id, mat_name));
                continue; // No triangles for this material
            }
            
            // Skip materials with bounds smaller than 1 voxel (too tiny to render)
            if let Some(b) = bounds {
                let size_x = (b.max.x - b.min.x).abs();
                let size_y = (b.max.y - b.min.y).abs();
                let size_z = (b.max.z - b.min.z).abs();
                if size_x < 0.5 && size_y < 0.5 && size_z < 0.5 {
                    skipped_count += 1;
                    debug_log(&opts.logger, format!("Material {} ({}) - SKIPPED: too small ({:.2}x{:.2}x{:.2})", 
                        mat_id, mat_name, size_x, size_y, size_z));
                    continue;
                }
                // Log bounds info for materials we're processing
                debug_log(&opts.logger, format!("Material {} ({}) bounds: min({:.1},{:.1},{:.1}) max({:.1},{:.1},{:.1}) size({:.1}x{:.1}x{:.1})", 
                    mat_id, mat_name, b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z, size_x, size_y, size_z));
            } else {
                debug_log(&opts.logger, format!("Material {} ({}) - WARNING: no bounds found", mat_id, mat_name));
            }
            
            let triangle_count = triangles.map(|t| t.len()).unwrap_or(0);
            
            let mat_start = std::time::Instant::now();
            let base_progress = 10 + (mat_id * 80 / material_count) as u32;
            
            // Calculate ETA based on average processing time
            let materials_remaining = material_count - mat_id - 1;
            let eta_seconds = (materials_remaining as f32 * avg_time_per_material).max(0.0);
            let eta_text = if eta_seconds < 60.0 {
                format!(" (est. {}s left)", eta_seconds.ceil() as u32)
            } else {
                let minutes = (eta_seconds / 60.0).floor() as u32;
                let seconds = (eta_seconds % 60.0).ceil() as u32;
                format!(" (est. {}m {}s left)", minutes, seconds)
            };
            
            set_progress(opts, base_progress, &format!("Processing material {} ({}, {} tris) of {}{}", 
                mat_id + 1, mat_name, triangle_count, material_count, eta_text));

            let (brick_material, brick_intensity) = material_mappings[mat_id];

            // Voxelize using pre-grouped triangles (much faster than re-parsing models)
            // Returns (octree, world_offset) - we need to add world_offset back to brick positions
            let (mut octree, world_offset) = voxelize_from_pregrouped(&pregrouped, &material_images, mat_id, None, Some(&opts.logger));
            
            // Log voxelization results
            debug_log(&opts.logger, format!("Material {} ({}) voxelized: octree size {}, offset({:.1},{:.1},{:.1})", 
                mat_id, mat_name, octree.size, world_offset.x, world_offset.y, world_offset.z));

            let max_merge = 500;
            let mut save_data = SaveData {
                bricks: Vec::new(),
                colors: palette::DEFAULT_PALETTE.to_vec(),
                author_name: opts.save_owner_name.clone(),
            };

            if opts.simplify {
                simplify_lossy_with_material(&mut octree, &mut save_data, opts, max_merge, brick_material, brick_intensity);
            } else {
                simplify_lossless_with_material(&mut octree, &mut save_data, opts, max_merge, brick_material, brick_intensity);
            }
            
            // Apply world offset to restore original positions
            // Bricks were created in local coordinates, need to translate back to world space
            // Brick position formula: scale * size + 2 * scale * pos
            // So offset needs to be multiplied by 2 * scale to match the pos term
            let brick_scale_factor = 2.0 * opts.brick_scale as f32;
            
            // Calculate brick position ranges before and after offset
            let mut min_pos_before = (i32::MAX, i32::MAX, i32::MAX);
            let mut max_pos_before = (i32::MIN, i32::MIN, i32::MIN);
            for brick in &save_data.bricks {
                min_pos_before.0 = min_pos_before.0.min(brick.position.x);
                min_pos_before.1 = min_pos_before.1.min(brick.position.y);
                min_pos_before.2 = min_pos_before.2.min(brick.position.z);
                max_pos_before.0 = max_pos_before.0.max(brick.position.x);
                max_pos_before.1 = max_pos_before.1.max(brick.position.y);
                max_pos_before.2 = max_pos_before.2.max(brick.position.z);
            }
            
            for brick in &mut save_data.bricks {
                brick.position.x += (world_offset.x * brick_scale_factor) as i32;
                brick.position.y += (world_offset.y * brick_scale_factor) as i32;
                brick.position.z += (world_offset.z * brick_scale_factor) as i32;
            }
            
            // Calculate brick position ranges after world offset
            let mut min_pos_after = (i32::MAX, i32::MAX, i32::MAX);
            let mut max_pos_after = (i32::MIN, i32::MIN, i32::MIN);
            for brick in &save_data.bricks {
                min_pos_after.0 = min_pos_after.0.min(brick.position.x);
                min_pos_after.1 = min_pos_after.1.min(brick.position.y);
                min_pos_after.2 = min_pos_after.2.min(brick.position.z);
                max_pos_after.0 = max_pos_after.0.max(brick.position.x);
                max_pos_after.1 = max_pos_after.1.max(brick.position.y);
                max_pos_after.2 = max_pos_after.2.max(brick.position.z);
            }
            
            if crate::DEBUG_MODE && !save_data.bricks.is_empty() {
                debug_log(&opts.logger, format!("Brick positions before offset: X[{},{}] Y[{},{}] Z[{},{}]",
                    min_pos_before.0, max_pos_before.0, min_pos_before.1, max_pos_before.1, min_pos_before.2, max_pos_before.2));
                debug_log(&opts.logger, format!("Brick positions after world offset: X[{},{}] Y[{},{}] Z[{},{}]",
                    min_pos_after.0, max_pos_after.0, min_pos_after.1, max_pos_after.1, min_pos_after.2, max_pos_after.2));
            }
            
            // Auto-correct negative brick positions by shifting all bricks to positive space
            // Brickadia frozen grids don't render bricks at negative local positions correctly
            if !save_data.bricks.is_empty() {
                let correction_x = if min_pos_after.0 < 0 { -min_pos_after.0 } else { 0 };
                let correction_y = if min_pos_after.1 < 0 { -min_pos_after.1 } else { 0 };
                let correction_z = if min_pos_after.2 < 0 { -min_pos_after.2 } else { 0 };
                
                if correction_x > 0 || correction_y > 0 || correction_z > 0 {
                    debug_log(&opts.logger, format!("Auto-correcting negative positions: shifting by ({}, {}, {})",
                        correction_x, correction_y, correction_z));
                    
                    for brick in &mut save_data.bricks {
                        brick.position.x += correction_x;
                        brick.position.y += correction_y;
                        brick.position.z += correction_z;
                    }
                    
                    // Update min/max after correction
                    min_pos_after.0 += correction_x;
                    max_pos_after.0 += correction_x;
                    min_pos_after.1 += correction_y;
                    max_pos_after.1 += correction_y;
                    min_pos_after.2 += correction_z;
                    max_pos_after.2 += correction_z;
                    
                    debug_log(&opts.logger, format!("Brick positions after auto-correction: X[{},{}] Y[{},{}] Z[{},{}]",
                        min_pos_after.0, max_pos_after.0, min_pos_after.1, max_pos_after.1, min_pos_after.2, max_pos_after.2));
                }
            }

            let mat_elapsed = mat_start.elapsed();
            
            // Update running average of processing time
            total_processing_time += mat_elapsed.as_secs_f32();
            let materials_processed_so_far = mat_id + 1 - skipped_count;
            if materials_processed_so_far > 0 {
                avg_time_per_material = total_processing_time / materials_processed_so_far as f32;
            }
            
            if !save_data.bricks.is_empty() {
                processed_count += 1;
                opts.logger.log(format!("Material {} ({}, {} tris) -> {} bricks as {:?} in {:.2}s", 
                    mat_id, mat_name, triangle_count, save_data.bricks.len(), brick_material, mat_elapsed.as_secs_f32()));

                // Brick positions are kept as world coordinates
                // Entity location is used as an additional offset (for separating multiple material grids)
                // The write_brz_grids function will combine entity location + brick position
                
                if opts.group_by_brick_material {
                    // Consolidate bricks by Brickadia material type
                    bricks_by_type.entry(brick_material).or_default().extend(save_data.bricks);
                } else {
                    // One grid per texture material (original behavior)
                    // Only apply offset multiplier if user has set a non-zero grid offset
                    // Otherwise keep all materials at origin so they stay aligned
                    let has_offset = opts.grid_offset_x != 0.0 || opts.grid_offset_y != 0.0 || opts.grid_offset_z != 0.0;
                    let offset_multiplier = if has_offset { mat_id as f32 } else { 0.0 };
                    
                    let entity = Entity {
                        frozen: false,
                        location: brdb::Vector3f {
                            x: opts.grid_offset_x * offset_multiplier,
                            y: opts.grid_offset_y * offset_multiplier,
                            z: opts.grid_offset_z * offset_multiplier,
                        },
                        ..Default::default()
                    };
                    material_grids.push((entity, save_data.bricks));
                }
            } else {
                debug_log(&opts.logger, format!("Material {} ({}) -> 0 bricks in {:.2}s (skipped)", 
                    mat_id, mat_name, mat_elapsed.as_secs_f32()));
            }
        }

        // If grouping by brick material type, create grids from the consolidated buckets
        if opts.group_by_brick_material {
            let mut type_index = 0;
            for (brick_material, bricks) in bricks_by_type {
                // Brick positions are already in world coordinates
                // Entity location provides optional offset for separating material types
                opts.logger.log(format!("Grid {:?} -> {} bricks", brick_material, bricks.len()));
                
                let entity = Entity {
                    frozen: false,
                    location: brdb::Vector3f {
                        x: opts.grid_offset_x * type_index as f32,
                        y: opts.grid_offset_y * type_index as f32,
                        z: opts.grid_offset_z * type_index as f32,
                    },
                    ..Default::default()
                };
                material_grids.push((entity, bricks));
                type_index += 1;
            }
            opts.logger.log(format!("Consolidated {} texture materials into {} grids by Brickadia material type", 
                processed_count, material_grids.len()));
        }

        // Log summary
        opts.logger.log(format!("Processed {} materials -> {} grids ({} skipped early)", 
            material_count, material_grids.len(), skipped_count));

        // Save material cache if modified
        if let Some(ref mapping) = cached_mapping {
            if let Err(e) = mapping.save_cache() {
                opts.logger.log(format!("Warning: Failed to save material cache: {}", e));
            } else {
                opts.logger.log(format!("Saved material cache ({} entries)", mapping.cache_size()));
            }
        }

        // Add origin marker if enabled
        if opts.show_origin_marker {
            let marker_bricks = create_origin_marker_bricks(opts);
            let marker_entity = Entity {
                frozen: false,
                location: brdb::Vector3f { x: 0.0, y: 0.0, z: 0.0 },
                ..Default::default()
            };
            material_grids.push((marker_entity, marker_bricks));
        }

        set_progress(opts, 90, "Writing BRZ file...");
        write_brz_with_grids(opts, material_grids)
    } else {
        // Regular single-grid conversion
        set_progress(opts, 10, "Loading model...");
        let mut octree = generate_octree(opts, skip_textures, None)?;
        
        if is_cancelled(opts) {
            opts.logger.log("Conversion cancelled.".to_string());
            return Ok(());
        }
        
        set_progress(opts, 50, "Simplifying and generating bricks...");
        write_brz_data(&mut octree, opts, None)
    }
}

fn load_models_and_materials(
    opt: &Obj2Brs,
    skip_textures: bool,
) -> ConversionResult<(Vec<tobj::Model>, Vec<image::RgbaImage>, Vec<String>)> {
    let p = Path::new(&opt.input_file_path);

    opt.logger.log("Importing model...".to_string());
    debug_log(&opt.logger, format!("Input file: {:?}", p));
    debug_log(&opt.logger, format!("Skip textures: {}", skip_textures));
    
    let load_options = LoadOptions {
        triangulate: true,
        ignore_lines: true,
        ignore_points: true,
        single_index: true,
    };
    let (mut models, materials) = tobj::load_obj(&opt.input_file_path, &load_options)
        .map_err(|e| ConversionError::ObjParseError(e.to_string()))?;

    // Log model statistics
    debug_log(&opt.logger, format!("Loaded {} model(s)", models.len()));
    if VERBOSE_MODEL_LOADING {
        for (i, model) in models.iter().enumerate() {
            let vertex_count = model.mesh.positions.len() / 3;
            let face_count = model.mesh.indices.len() / 3;
            debug_log(&opt.logger, format!("  Model {}: '{}' - {} vertices, {} faces", 
                i, model.name, vertex_count, face_count));
        }
    }

    opt.logger.log("Loading materials...".to_string());
    let mut material_images = Vec::<image::RgbaImage>::new();
    let mut material_names = Vec::<String>::new();

    let materials = materials.unwrap_or_else(|_| Vec::new());

    if materials.is_empty() {
        opt.logger.log("  No materials found, using default white color".to_string());
        material_images.push(create_solid_color_texture([1.0, 1.0, 1.0], 1.0));
        material_names.push("default".to_string());
    } else {
        for material in materials {
            material_names.push(material.name.clone());
            // Try to load texture if available and not skipping
            if !skip_textures {
                if let Some(ref texture_name) = material.diffuse_texture {
                    if texture_name.is_empty() {
                        // Empty texture name, use material color
                        let diffuse = material.diffuse.unwrap_or([1.0, 1.0, 1.0]);
                        let dissolve = material.dissolve.unwrap_or(1.0);
                        material_images.push(create_solid_color_texture(diffuse, dissolve));
                        continue;
                    }
                    let image_path = p.parent()
                        .ok_or_else(|| ConversionError::ObjFileNotFound { path: p.to_path_buf() })?
                        .join(texture_name);

                    if VERBOSE_TEXTURE_LOADING {
                        opt.logger.log(format!(
                            "  Loading diffuse texture for {} from: {}",
                            material.name, image_path.display()
                        ));
                    }

                    // Try to load texture
                    match image::open(&image_path) {
                        Ok(img) => {
                            material_images.push(img.into_rgba8());
                        }
                        Err(e) => {
                            return Err(ConversionError::TextureLoadError {
                                path: image_path,
                                reason: e.to_string(),
                            });
                        }
                    }
                } else {
                    // No texture - always log missing textures
                    opt.logger.log(format!(
                        "  Material {} does not have a texture, using material color",
                        material.name
                    ));
                    let diffuse = material.diffuse.unwrap_or([1.0, 1.0, 1.0]);
                    let dissolve = material.dissolve.unwrap_or(1.0);
                    material_images.push(create_solid_color_texture(diffuse, dissolve));
                }
            } else {
                // Skipping textures, use material color
                if VERBOSE_TEXTURE_LOADING {
                    opt.logger.log(format!(
                        "  Skipping textures for material {}, using material color",
                        material.name
                    ));
                }
                let diffuse = material.diffuse.unwrap_or([1.0, 1.0, 1.0]);
                let dissolve = material.dissolve.unwrap_or(1.0);
                material_images.push(create_solid_color_texture(diffuse, dissolve));
            }
        }
    }

    // Check for large coordinate ranges and warn user
    check_model_bounds(&models, opt);

    // Apply rotation if any rotation is set
    if opt.rotation_x != 0 || opt.rotation_y != 0 || opt.rotation_z != 0 {
        rotate_models(&mut models, opt.rotation_x, opt.rotation_y, opt.rotation_z);
        opt.logger.log(format!(
            "Applied rotation: X={}°, Y={}°, Z={}°",
            opt.rotation_x, opt.rotation_y, opt.rotation_z
        ));
    }

    // Scale models (also centers at origin)
    scale_models(&mut models, opt.scale, opt.scale_x, opt.scale_y, opt.scale_z, &opt.logger);

    Ok((models, material_images, material_names))
}

fn check_model_bounds(models: &[tobj::Model], opt: &Obj2Brs) {
    if let Some(first_model) = models.first() {
        let positions = &first_model.mesh.positions;
        if !positions.is_empty() {
            let mut min_x = positions[0];
            let mut max_x = positions[0];
            let mut min_y = positions[1];
            let mut max_y = positions[1];
            let mut min_z = positions[2];
            let mut max_z = positions[2];

            for m in models.iter() {
                let p = &m.mesh.positions;
                for v in (0..p.len()).step_by(3) {
                    min_x = min_x.min(p[v]);
                    max_x = max_x.max(p[v]);
                    min_y = min_y.min(p[v + 1]);
                    max_y = max_y.max(p[v + 1]);
                    min_z = min_z.min(p[v + 2]);
                    max_z = max_z.max(p[v + 2]);
                }
            }

            let range_x = max_x - min_x;
            let range_y = max_y - min_y;
            let range_z = max_z - min_z;
            let max_range = range_x.max(range_y).max(range_z);

            debug_log(&opt.logger, format!(
                "Model bounds: X[{:.1}, {:.1}] Y[{:.1}, {:.1}] Z[{:.1}, {:.1}]",
                min_x, max_x, min_y, max_y, min_z, max_z
            ));
            debug_log(&opt.logger, format!(
                "Model size: {:.1} x {:.1} x {:.1} units (max: {:.1})",
                range_x, range_y, range_z, max_range
            ));

            // Warn if model has very large coordinates (typical for BSP maps)
            if max_range > 10000.0 {
                opt.logger.log(format!(
                    "WARNING: Model is very large ({:.0} units). This may require significant memory.",
                    max_range
                ));
                opt.logger.log("Model will be centered at origin before scaling to optimize memory usage.".to_string());
            } else if max_range > 5000.0 {
                opt.logger.log(format!(
                    "Note: Large model detected ({:.0} units). Centering at origin for better memory efficiency.",
                    max_range
                ));
            }
        }
    }
}

/// Rotate models around X, Y, Z axes by the given angles (in degrees, must be 0, 90, 180, or 270).
/// 
/// The UI describes rotations in Brickadia coordinates (Z-up), but OBJ files use Y-up.
/// Since the simplify code swaps Y<->Z when creating bricks, we need to swap the rotation axes:
/// - UI "Rotation X" (Brickadia left-right) -> OBJ X axis
/// - UI "Rotation Y" (Brickadia forward-back) -> OBJ Z axis (swapped)
/// - UI "Rotation Z" (Brickadia up-down) -> OBJ Y axis (swapped)
///
/// Rotation is applied in order: X, then Y (mapped to OBJ Z), then Z (mapped to OBJ Y).
/// The model is first centered at the origin, rotated, then the center is preserved.
fn rotate_models(models: &mut [tobj::Model], rot_x: i32, rot_y: i32, rot_z: i32) {
    // Map Brickadia axes to OBJ axes (Y<->Z swap)
    let obj_rot_x = rot_x;  // X stays the same
    let obj_rot_y = rot_z;  // Brickadia Z (up) -> OBJ Y (up)
    let obj_rot_z = rot_y;  // Brickadia Y (forward) -> OBJ Z (forward)
    
    // First, find the center of the model's bounding box
    let (center_x, center_y, center_z) = if let Some(first_model) = models.first() {
        let positions = &first_model.mesh.positions;
        if !positions.is_empty() {
            let mut min_x = positions[0];
            let mut max_x = positions[0];
            let mut min_y = positions[1];
            let mut max_y = positions[1];
            let mut min_z = positions[2];
            let mut max_z = positions[2];

            for m in models.iter() {
                let p = &m.mesh.positions;
                for v in (0..p.len()).step_by(3) {
                    min_x = min_x.min(p[v]);
                    max_x = max_x.max(p[v]);
                    min_y = min_y.min(p[v + 1]);
                    max_y = max_y.max(p[v + 1]);
                    min_z = min_z.min(p[v + 2]);
                    max_z = max_z.max(p[v + 2]);
                }
            }
            ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0, (min_z + max_z) / 2.0)
        } else {
            (0.0, 0.0, 0.0)
        }
    } else {
        return;
    };

    // Apply rotation to each vertex
    for m in models.iter_mut() {
        let p = &mut m.mesh.positions;
        for v in (0..p.len()).step_by(3) {
            // Translate to origin
            let mut x = p[v] - center_x;
            let mut y = p[v + 1] - center_y;
            let mut z = p[v + 2] - center_z;

            // Rotate around OBJ X axis (Brickadia X - left/right)
            if obj_rot_x != 0 {
                let (new_y, new_z) = rotate_2d(y, z, obj_rot_x);
                y = new_y;
                z = new_z;
            }

            // Rotate around OBJ Y axis (Brickadia Z - up/down, spin in place)
            if obj_rot_y != 0 {
                let (new_x, new_z) = rotate_2d(x, z, obj_rot_y);
                x = new_x;
                z = new_z;
            }

            // Rotate around OBJ Z axis (Brickadia Y - forward/back)
            if obj_rot_z != 0 {
                let (new_x, new_y) = rotate_2d(x, y, obj_rot_z);
                x = new_x;
                y = new_y;
            }

            // Translate back (keep centered for now, scale_models will re-center)
            p[v] = x + center_x;
            p[v + 1] = y + center_y;
            p[v + 2] = z + center_z;
        }
    }
}

/// Rotate a 2D point (a, b) by the given angle in degrees (0, 90, 180, 270).
/// Returns the rotated point.
#[inline]
fn rotate_2d(a: f32, b: f32, degrees: i32) -> (f32, f32) {
    match degrees % 360 {
        90 | -270 => (-b, a),
        180 | -180 => (-a, -b),
        270 | -90 => (b, -a),
        _ => (a, b), // 0 degrees or invalid
    }
}

fn scale_models(models: &mut [tobj::Model], scale: f32, scale_x: f32, scale_y: f32, scale_z: f32, logger: &Logger) {
    // Apply base scale plus per-axis scale multipliers.
    // Note: scale_x/y/z are in Brickadia coordinates, but OBJ uses Y-up.
    // Since simplify swaps Y↔Z, we need to swap scale_y and scale_z here.
    let final_scale_x = scale * scale_x;  // Brickadia X = OBJ X
    let final_scale_y = scale * scale_z;  // Brickadia Z (up) = OBJ Y (up)
    let final_scale_z = scale * scale_y;  // Brickadia Y (forward) = OBJ Z (forward)

    // First, find the center of the model's bounding box
    if let Some(first_model) = models.first() {
        let positions = &first_model.mesh.positions;
        if !positions.is_empty() {
            let mut min_x = positions[0];
            let mut max_x = positions[0];
            let mut min_y = positions[1];
            let mut max_y = positions[1];
            let mut min_z = positions[2];
            let mut max_z = positions[2];

            for m in models.iter() {
                let p = &m.mesh.positions;
                for v in (0..p.len()).step_by(3) {
                    min_x = min_x.min(p[v]);
                    max_x = max_x.max(p[v]);
                    min_y = min_y.min(p[v + 1]);
                    max_y = max_y.max(p[v + 1]);
                    min_z = min_z.min(p[v + 2]);
                    max_z = max_z.max(p[v + 2]);
                }
            }

            // Calculate center offset
            let center_x = (min_x + max_x) / 2.0;
            let center_y = (min_y + max_y) / 2.0;
            let center_z = (min_z + max_z) / 2.0;

            // Translate model to origin (center it)
            for m in models.iter_mut() {
                let p = &mut m.mesh.positions;
                for v in (0..p.len()).step_by(3) {
                    p[v] -= center_x;
                    p[v + 1] -= center_y;
                    p[v + 2] -= center_z;
                }
            }
        }
    }

    // Now apply scaling
    for m in models.iter_mut() {
        let p = &mut m.mesh.positions;
        for v in (0..p.len()).step_by(3) {
            p[v] *= final_scale_x;
            p[v + 1] *= final_scale_y;
            p[v + 2] *= final_scale_z;
        }
    }

    // Offset mesh so no vertices have negative coordinates
    // This is critical because Brickadia doesn't handle negative brick positions correctly
    if let Some(first_model) = models.first() {
        let positions = &first_model.mesh.positions;
        if !positions.is_empty() {
            let mut min_x = positions[0];
            let mut min_y = positions[1];
            let mut min_z = positions[2];
            
            for m in models.iter() {
                let p = &m.mesh.positions;
                for v in (0..p.len()).step_by(3) {
                    min_x = min_x.min(p[v]);
                    min_y = min_y.min(p[v + 1]);
                    min_z = min_z.min(p[v + 2]);
                }
            }

            // Calculate offsets needed to make all coordinates non-negative
            let x_offset = if min_x < 0.0 { -min_x } else { 0.0 };
            let y_offset = if min_y < 0.0 { -min_y } else { 0.0 };
            let z_offset = if min_z < 0.0 { -min_z } else { 0.0 };

            // Apply offsets if any axis has negative values
            if x_offset > 0.0 || y_offset > 0.0 || z_offset > 0.0 {
                debug_log(logger, format!(
                    "Offsetting model to eliminate negative coordinates: X+{:.1}, Y+{:.1}, Z+{:.1}",
                    x_offset, y_offset, z_offset
                ));
                for m in models.iter_mut() {
                    let p = &mut m.mesh.positions;
                    for v in (0..p.len()).step_by(3) {
                        p[v] += x_offset;
                        p[v + 1] += y_offset;
                        p[v + 2] += z_offset;
                    }
                }
            }
        }
    }
}

fn voxelize_models(
    models: &mut [tobj::Model],
    material_images: &[image::RgbaImage],
    opts: &Obj2Brs,
    material_filter: Option<usize>,
) -> octree::VoxelTree<Vector4<u8>> {
    // Calculate model statistics for progress estimation
    let total_models = models.len();
    let mut total_vertices = 0usize;
    let mut total_faces = 0usize;
    for m in models.iter() {
        total_vertices += m.mesh.positions.len() / 3;
        total_faces += m.mesh.indices.len() / 3;
    }
    
    if let Some(filter_id) = material_filter {
        opts.logger.log(format!("Voxelizing material {}...", filter_id));
    } else {
        opts.logger.log(format!(
            "Voxelizing {} models ({} vertices, {} faces)...",
            total_models, total_vertices, total_faces
        ));
    }
    
    debug_log(&opts.logger, format!("Scale: {}, Brick type: {:?}", opts.scale, opts.bricktype));
    debug_log(&opts.logger, format!("Material filter: {:?}", material_filter));
    debug_log(&opts.logger, format!("Material images count: {}", material_images.len()));
    
    // Create progress tracker
    let progress = VoxelizeProgress::new();
    let progress_clone = progress.clone();
    let logger_clone = opts.logger.clone();
    
    // Heartbeat thread to report progress every 5 seconds
    let heartbeat_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let heartbeat_flag = heartbeat_running.clone();
    let heartbeat_handle = thread::spawn(move || {
        let mut last_processed = 0usize;
        let mut last_rate_samples: Vec<usize> = Vec::with_capacity(6); // Keep last 30s of rates
        let start_time = std::time::Instant::now();
        while heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed) {
            thread::sleep(std::time::Duration::from_secs(5));
            if !heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let processed = progress_clone.triangles_processed.load(std::sync::atomic::Ordering::Relaxed);
            let _total = progress_clone.triangles_total.load(std::sync::atomic::Ordering::Relaxed);
            let depth = progress_clone.depth_current.load(std::sync::atomic::Ordering::Relaxed);
            let max_depth = progress_clone.depth_max.load(std::sync::atomic::Ordering::Relaxed);
            let elapsed = start_time.elapsed();
            
            let delta = processed.saturating_sub(last_processed);
            last_processed = processed;
            
            // Track rate samples for smoothed estimation
            let current_rate = delta / 5; // per second
            if last_rate_samples.len() >= 6 {
                last_rate_samples.remove(0);
            }
            last_rate_samples.push(current_rate);
            
            // Calculate smoothed average rate
            let avg_rate = if !last_rate_samples.is_empty() {
                last_rate_samples.iter().sum::<usize>() / last_rate_samples.len()
            } else {
                current_rate
            };
            
            // Format large numbers with commas for readability
            let processed_fmt = format_number(processed);
            let delta_fmt = format_number(delta);
            let rate_fmt = format_number(avg_rate);
            
            logger_clone.log(format!(
                "  [Voxelizing] {} voxels (+{}/5s, ~{}/s), depth {}/{}, elapsed {:.0?}",
                processed_fmt, delta_fmt, rate_fmt, max_depth.saturating_sub(depth), max_depth, elapsed
            ));
        }
    });
    
    let start = std::time::Instant::now();
    let result = voxelize_with_progress(models, material_images, opts.scale, opts.bricktype, material_filter, Some(progress.clone()), Some(&opts.logger));
    let elapsed = start.elapsed();
    
    // Stop heartbeat thread
    heartbeat_running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = heartbeat_handle.join();
    
    let final_voxels = progress.triangles_processed.load(std::sync::atomic::Ordering::Relaxed);
    let voxels_fmt = format_number(final_voxels);
    let rate = if elapsed.as_secs() > 0 { final_voxels / elapsed.as_secs() as usize } else { final_voxels };
    let rate_fmt = format_number(rate);
    opts.logger.log(format!("Voxelization completed: {} voxels in {:.2?} (~{}/s avg)", voxels_fmt, elapsed, rate_fmt));
    
    // Log octree size and check if it's too large for simplification
    let grid_size = 1u64 << (result.size + 1);
    debug_log(&opts.logger, format!("Octree size: {} (grid dimensions: {}³)", result.size, grid_size));
    
    // Warn if octree is very large (will cause issues during simplification)
    let grid_volume = grid_size.pow(3);
    let bytes_per_voxel = std::mem::size_of::<Option<cgmath::Vector4<u8>>>() as u64;
    let estimated_bytes = grid_volume.saturating_mul(bytes_per_voxel);
    let estimated_gb = estimated_bytes as f64 / 1_073_741_824.0;
    
    if estimated_gb > 16.0 {
        opts.logger.log(format!(
            "WARNING: Octree is very large ({}^3 grid = {:.1} GB for simplification).",
            grid_size, estimated_gb
        ));
        opts.logger.log("Simplification will be skipped to prevent memory overflow.".to_string());
    }
    
    result
}

fn generate_octree(opt: &Obj2Brs, skip_textures: bool, material_filter: Option<usize>) -> ConversionResult<octree::VoxelTree<Vector4<u8>>> {
    opt.logger.log(format!("Loading {}", Path::new(&opt.input_file_path).display()));
    let (mut models, material_images, _material_names) = load_models_and_materials(opt, skip_textures)?;
    Ok(voxelize_models(&mut models, &material_images, opt, material_filter))
}

fn write_brz_data(octree: &mut octree::VoxelTree<Vector4<u8>>, opts: &Obj2Brs, material_id: Option<usize>) -> ConversionResult<()> {
    let max_merge = 500;

    let mut save_data = SaveData {
        bricks: Vec::new(),
        colors: palette::DEFAULT_PALETTE.to_vec(),
        author_name: opts.save_owner_name.clone(),
    };

    // Check if octree is too large for simplification (would cause memory overflow)
    // VoxelGrid allocates size³ where size = 2^(octree.size+1)
    // We need to ensure size³ * sizeof(Option<Vector4<u8>>) < reasonable memory limit
    let grid_size = 1u64 << (octree.size + 1);
    let grid_volume = grid_size.pow(3);
    let bytes_per_voxel = std::mem::size_of::<Option<Vector4<u8>>>() as u64;
    let estimated_bytes = grid_volume.saturating_mul(bytes_per_voxel);
    let estimated_gb = estimated_bytes as f64 / 1_073_741_824.0;
    
    debug_log(&opts.logger, format!("Simplification memory estimate: {:.2} GB for {}³ grid", estimated_gb, grid_size));
    
    // If estimated memory > 16 GB, skip simplification and use direct brick generation
    if estimated_gb > 16.0 {
        opts.logger.log(format!(
            "WARNING: Model is too large for simplification ({:.1} GB required).",
            estimated_gb
        ));
        opts.logger.log("Generating bricks directly from octree (1 brick per voxel).".to_string());
        opts.logger.log("Tip: Use a larger scale value to reduce brick count.".to_string());
        
        // Generate bricks directly without VoxelGrid allocation
        simplify_direct::generate_bricks_direct(octree, &mut save_data, opts);
    } else {
        set_progress(opts, 60, "Simplifying... (this may take 5-10 minutes for complex models)");
        if let Some(id) = material_id {
            opts.logger.log(format!("Simplifying material {}... (please wait, this can take several minutes)", id));
        } else {
            opts.logger.log("Simplifying... (please wait, this can take 5-10 minutes for complex models)".to_string());
        }

        debug_log(&opts.logger, format!("Simplify mode: {}", if opts.simplify { "lossy" } else { "lossless" }));
        debug_log(&opts.logger, format!("Max merge: {}", max_merge));
        
        let start = std::time::Instant::now();
        if opts.simplify {
            simplify_lossy(octree, &mut save_data, opts, max_merge);
        } else {
            simplify_lossless(octree, &mut save_data, opts, max_merge);
        }
        let elapsed = start.elapsed();
        
        debug_log(&opts.logger, format!("Simplification completed in {:.2?}", elapsed));
    }
    debug_log(&opts.logger, format!("Generated {} bricks", save_data.bricks.len()));
    debug_log(&opts.logger, format!("Using {} colors", save_data.colors.len()));

    // Add origin marker if enabled
    if opts.show_origin_marker {
        add_origin_marker(&mut save_data, opts);
    }

    // Apply color merging if enabled
    if opts.color_merge_threshold > 0.0 {
        set_progress(opts, 92, "Merging similar colors...");
        color_merge::merge_similar_colors(&mut save_data, opts.color_merge_threshold, &opts.logger);
    }

    set_progress(opts, 95, "Writing file...");
    opts.logger.log(format!("Writing {} bricks...", save_data.bricks.len()));

    let preview = image::load_from_memory_with_format(OBJ_ICON, image::ImageFormat::Png)
        .map_err(|e| ConversionError::SaveWriteError(format!("Failed to load preview icon: {}", e)))?;

    // Convert preview to Jpeg for BRZ
    let mut preview_bytes_jpg = Vec::new();
    preview
        .write_to(&mut Cursor::new(&mut preview_bytes_jpg), image::ImageOutputFormat::Jpeg(85))
        .map_err(|e| ConversionError::SaveWriteError(format!("Failed to encode JPEG preview: {}", e)))?;

    let output_file_path =
        PathBuf::from(&opts.output_directory).join(opts.save_name.clone() + ".brz");

    // Determine if we should use procedural bricks based on brick type
    let use_procedural = opts.bricktype != BrickType::Default;

    brdb_support::write_brz(
        output_file_path.clone(),
        &save_data,
        opts,
        use_procedural,
        Some(preview_bytes_jpg),
    )?;

    opts.logger.log(format!("Save written to: {}", output_file_path.display()));
    Ok(())
}

fn write_brz_with_grids(opts: &Obj2Brs, mut grids: Vec<(Entity, Vec<Brick>)>) -> ConversionResult<()> {
    opts.logger.log(format!("Writing {} frozen grids...", grids.len()));

    // Apply color merging to each grid if enabled
    if opts.color_merge_threshold > 0.0 {
        set_progress(opts, 92, "Merging similar colors across grids...");
        let grid_count = grids.len();
        for (idx, (_, bricks)) in grids.iter_mut().enumerate() {
            let mut save_data = SaveData {
                bricks: std::mem::take(bricks),
                colors: palette::DEFAULT_PALETTE.to_vec(),
                author_name: opts.save_owner_name.clone(),
            };
            color_merge::merge_similar_colors(&mut save_data, opts.color_merge_threshold, &opts.logger);
            *bricks = save_data.bricks;
            
            if (idx + 1) % 10 == 0 {
                opts.logger.log(format!("Processed {}/{} grids for color merging", idx + 1, grid_count));
            }
        }
    }

    let preview = image::load_from_memory_with_format(OBJ_ICON, image::ImageFormat::Png)
        .map_err(|e| ConversionError::SaveWriteError(format!("Failed to load preview icon: {}", e)))?;

    // Convert preview to Jpeg for BRZ
    let mut preview_bytes_jpg = Vec::new();
    preview
        .write_to(&mut Cursor::new(&mut preview_bytes_jpg), image::ImageOutputFormat::Jpeg(85))
        .map_err(|e| ConversionError::SaveWriteError(format!("Failed to encode JPEG preview: {}", e)))?;

    let output_file_path =
        PathBuf::from(&opts.output_directory).join(opts.save_name.clone() + ".brz");

    brdb_support::write_brz_grids(
        output_file_path.clone(),
        grids,
        opts,
        Some(preview_bytes_jpg),
    )?;

    opts.logger.log(format!("Save written to: {}", output_file_path.display()));
    Ok(())
}

/// Add origin marker bricks with color-coded XYZ axis indicators.
/// - White brick at origin (0,0,0)
/// - Red bricks along +X axis
/// - Green bricks along +Y axis  
/// - Blue bricks along +Z axis
fn add_origin_marker(save_data: &mut SaveData, opts: &Obj2Brs) {
    use brdb::{Brick, BrickSize, BrickType as BrdbBrickType, Color, Direction, Position, Rotation};

    // Determine brick size based on brick type
    let (brick_size, unit_size) = if opts.bricktype == BrickType::Microbricks {
        let s = opts.brick_scale as u16;
        (BrickSize::new(s, s, s), opts.brick_scale as i32 * 2)
    } else {
        // Default/Tiles use 5x5x2 units
        (BrickSize::new(5, 5, 2), 10)
    };

    let asset_name = if opts.bricktype == BrickType::Microbricks {
        "PB_DefaultMicroBrick"
    } else if opts.bricktype == BrickType::Tiles || opts.use_smooth_bricks {
        "PB_DefaultSmoothTile"
    } else {
        "PB_DefaultBrick"
    };

    let brick_type = BrdbBrickType::from((asset_name, brick_size));

    let material_name = match opts.material {
        Material::Plastic => "BMC_Plastic",
        Material::Glass => "BMC_Glass",
        Material::Glow => "BMC_Glow",
        Material::Metallic => "BMC_Metallic",
        Material::Hologram => "BMC_Hologram",
        Material::Ghost => "BMC_Ghost",
    };

    // Colors: White (origin), Red (+X), Green (+Y), Blue (+Z)
    let white = Color::new(255, 255, 255);
    let red = Color::new(255, 0, 0);
    let green = Color::new(0, 255, 0);
    let blue = Color::new(0, 0, 255);

    // Helper to create a marker brick
    let create_marker = |x: i32, y: i32, z: i32, color: Color| -> Brick {
        Brick {
            id: None,
            asset: brick_type.clone(),
            owner_index: None,
            position: Position::new(x, y, z),
            rotation: Rotation::Deg0,
            direction: Direction::ZPositive,
            collision: Default::default(),
            visible: true,
            color,
            material: material_name.into(),
            material_intensity: opts.material_intensity as u8,
            components: Vec::new(),
        }
    };

    // Origin brick (white) at center
    save_data.bricks.push(create_marker(0, 0, unit_size / 2, white));

    // +X axis (red) - 5 bricks extending right
    for i in 1..=5 {
        save_data.bricks.push(create_marker(i * unit_size, 0, unit_size / 2, red));
    }

    // +Y axis (green) - 5 bricks extending forward
    for i in 1..=5 {
        save_data.bricks.push(create_marker(0, i * unit_size, unit_size / 2, green));
    }

    // +Z axis (blue) - 5 bricks extending up
    for i in 1..=5 {
        save_data.bricks.push(create_marker(0, 0, unit_size / 2 + i * unit_size, blue));
    }
}

/// Create origin marker bricks and return them as a Vec (for split-by-material path)
fn create_origin_marker_bricks(opts: &Obj2Brs) -> Vec<Brick> {
    use brdb::{Brick, BrickSize, BrickType as BrdbBrickType, Color, Direction, Position, Rotation};

    let mut bricks = Vec::new();

    // Determine brick size based on brick type
    let (brick_size, unit_size) = if opts.bricktype == BrickType::Microbricks {
        let s = opts.brick_scale as u16;
        (BrickSize::new(s, s, s), opts.brick_scale as i32 * 2)
    } else {
        // Default/Tiles use 5x5x2 units
        (BrickSize::new(5, 5, 2), 10)
    };

    let asset_name = if opts.bricktype == BrickType::Microbricks {
        "PB_DefaultMicroBrick"
    } else if opts.bricktype == BrickType::Tiles || opts.use_smooth_bricks {
        "PB_DefaultSmoothTile"
    } else {
        "PB_DefaultBrick"
    };

    let brick_type = BrdbBrickType::from((asset_name, brick_size));

    let material_name = match opts.material {
        Material::Plastic => "BMC_Plastic",
        Material::Glass => "BMC_Glass",
        Material::Glow => "BMC_Glow",
        Material::Metallic => "BMC_Metallic",
        Material::Hologram => "BMC_Hologram",
        Material::Ghost => "BMC_Ghost",
    };

    // Colors: White (origin), Red (+X), Green (+Y), Blue (+Z)
    let white = Color::new(255, 255, 255);
    let red = Color::new(255, 0, 0);
    let green = Color::new(0, 255, 0);
    let blue = Color::new(0, 0, 255);

    // Helper to create a marker brick
    let create_marker = |x: i32, y: i32, z: i32, color: Color| -> Brick {
        Brick {
            id: None,
            asset: brick_type.clone(),
            owner_index: None,
            position: Position::new(x, y, z),
            rotation: Rotation::Deg0,
            direction: Direction::ZPositive,
            collision: Default::default(),
            visible: true,
            color,
            material: material_name.into(),
            material_intensity: opts.material_intensity as u8,
            components: Vec::new(),
        }
    };

    // Origin brick (white) at center
    bricks.push(create_marker(0, 0, unit_size / 2, white));

    // +X axis (red) - 5 bricks extending right
    for i in 1..=5 {
        bricks.push(create_marker(i * unit_size, 0, unit_size / 2, red));
    }

    // +Y axis (green) - 5 bricks extending forward
    for i in 1..=5 {
        bricks.push(create_marker(0, i * unit_size, unit_size / 2, green));
    }

    // +Z axis (blue) - 5 bricks extending up
    for i in 1..=5 {
        bricks.push(create_marker(0, 0, unit_size / 2 + i * unit_size, blue));
    }

    bricks
}

fn main() {
    let logger = Logger::new();
    logger.log("obj2brz started - ready to convert OBJ files to Brickadia saves".to_string());

    // Log the data directory location
    let data_dir = logger::get_data_dir();
    let cache_dir = logger::get_cache_dir();
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
    let persistence_path = logger::get_cache_dir().join("app_state");
    
    let win_option = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
            .with_min_inner_size([500.0, 400.0])
            .with_resizable(true)
            .with_icon(egui::IconData {
                rgba: icon::ICON.to_vec(),
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
                eframe::get_value(storage, eframe::APP_KEY).unwrap_or_else(|| Obj2Brs {
                    output_directory: build_dir_clone.clone(),
                    logger: logger.clone(),
                    ..Default::default()
                })
            } else {
                Obj2Brs {
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
