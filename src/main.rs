mod barycentric;
mod brdb_support;
mod color;
mod error;
mod gui;
mod icon;
mod intersect;
mod logger;
mod octree;
mod palette;
mod simplify;
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
use simplify::*;
use std::{
    env, io::Cursor, ops::RangeInclusive, path::Path, path::PathBuf, sync::mpsc,
    sync::mpsc::Receiver, thread,
};
use tobj::LoadOptions;
use uuid::Uuid;
use voxelize::{voxelize_with_progress, VoxelizeProgress};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Obj2Brs {
    pub bricktype: BrickType,
    pub brick_scale: isize,
    #[serde(skip)]
    input_file_path_receiver: Option<Receiver<Option<PathBuf>>>,
    input_file_path: String,
    pub match_brickadia_colorset: bool,
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
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum BrickType {
    Microbricks,
    Default,
    Tiles,
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
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
            missing_resources_dialog: None,
            pending_conversion_skip_textures: false,
            logger: Logger::new(),
            conversion_in_progress: false,
            conversion_done_receiver: None,
            conversion_progress: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            conversion_stage: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
            conversion_cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            // BSP options
            input_file_type: InputFileType::Obj,
            bsp_game_source: GameSource::Auto,
            detected_game_source: None,
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
                // Progress bar (shown during conversion)
                if self.conversion_in_progress {
                    let progress = self.conversion_progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0;
                    let stage = self.conversion_stage.lock().map(|s| s.clone()).unwrap_or_default();
                    
                    ui.horizontal(|ui| {
                        ui.add(egui::ProgressBar::new(progress)
                            .show_percentage()
                            .animate(true));
                        if ui.button("Cancel").clicked() {
                            self.conversion_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                            self.logger.log("Cancellation requested...".to_string());
                        }
                    });
                    if !stage.is_empty() {
                        ui.label(RichText::new(&stage).color(egui::Color32::YELLOW).small());
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
                ui.horizontal(|ui| {
                    let available_width = ui.available_width();
                    if self.conversion_in_progress {
                        // Show Converting + Cancel buttons centered
                        ui.add_space((available_width - 160.0) / 2.0);
                        ui.add_enabled(false, egui::Button::new("Converting..."));
                        if ui.button("Cancel").clicked() {
                            self.conversion_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                            self.logger.log("Cancellation requested...".to_string());
                        }
                    } else {
                        // Show Voxelize button centered
                        ui.add_space((available_width - 60.0) / 2.0);
                        if gui::button(ui, "Voxelize", can_convert) {
                            self.do_conversion()
                        }
                    }
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
                if ui.button("📂").on_hover_text("Open output folder in file explorer").clicked() {
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
            "Whether or not to merge similar bricks to create a less detailed model",
        );
        ui.add(Checkbox::new(&mut self.simplify, "Simplify (reduces brickcount)"));
        ui.end_row();

        ui.label("Scale")
            .on_hover_text("Adjusts the overall size of the generated save");
        ui.add(
            DragValue::new(&mut self.scale)
                .min_decimals(2)
                .prefix("x")
                .speed(0.1),
        );
        ui.end_row();

        ui.label("Bricktype")
            .on_hover_text("Which type of bricks will make up the generated save, use default to get a stud texture");
        ComboBox::from_label("")
            .selected_text(format!("{:?}", &mut self.bricktype))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.bricktype, BrickType::Microbricks, "Microbricks");
                ui.selectable_value(&mut self.bricktype, BrickType::Default, "Default");
                ui.selectable_value(&mut self.bricktype, BrickType::Tiles, "Tiles");
            });
        ui.end_row();

        ui.label("Material");
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
    }

    fn advanced_options(&mut self, ui: &mut Ui, uuid_valid: bool) {
        ui.label("Material Intensity");
        ui.add(Slider::new(
            &mut self.material_intensity,
            RangeInclusive::new(0, 10),
        ));
        ui.end_row();

        ui.label("Match to Colorset").on_hover_text(
            "Modify the color of the model to match the default color palette in Brickadia",
        );
        ui.add(Checkbox::new(&mut self.match_brickadia_colorset, "Use Default Palette"));
        ui.end_row();

        ui.label("Split by Material (Experimental)").on_hover_text(
            "Process each OBJ material separately into frozen grids",
        );
        ui.add(Checkbox::new(&mut self.split_by_material, "Separate grids per material"));
        ui.end_row();

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

        if self.bricktype == BrickType::Microbricks {
            ui.label("Brick Scale")
                .on_hover_text("Use this to make microbricks bigger for a more pixelated look");
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
            if ui.button("📂").on_hover_text("Open cache folder").clicked() {
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
            if ui.button("🗑 Clear Cache").on_hover_text("Delete all cached data (settings will reset on next launch)").clicked() {
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
            if ui.button("📂").on_hover_text("Open data folder").clicked() {
                let path = data_path_str.clone();
                thread::spawn(move || {
                    let _ = open_folder_in_explorer(&path);
                });
            }
        });
    }

    fn show_missing_resources_dialog(&mut self, ctx: &egui::Context) {
        if let Some(message) = &self.missing_resources_dialog.clone() {
            let mut open = true;
            Window::new("⚠ Missing Resources")
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
        let match_brickadia_colorset = self.match_brickadia_colorset;
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
                logger.log(format!("[DEBUG] BSP temp directory: {:?}", temp_dir));
            }

            // Convert BSP to OBJ
            // Check for external texture directory (VTF files for GoldSrc)
            let texture_dir = Path::new("data/game_textures/goldsrc_textures/materials");
            let texture_dir_opt = if texture_dir.exists() {
                logger.log(format!("Using external textures from: {:?}", texture_dir));
                Some(texture_dir)
            } else {
                logger.log("No external texture directory found, using embedded/inferred colors".to_string());
                None
            };
            
            let bsp_result = bsp_converter::convert_bsp_to_obj_with_game_and_textures(
                &input_file_path,
                &temp_dir,
                bsp_game_source,
                texture_dir_opt,
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
                missing_resources_dialog: None,
                pending_conversion_skip_textures: false,
                logger: logger.clone(),
                conversion_in_progress: true,
                conversion_done_receiver: None,
                conversion_progress: progress.clone(),
                conversion_stage: stage.clone(),
                conversion_cancelled: cancelled.clone(),
                input_file_type: InputFileType::Obj,
                bsp_game_source: GameSource::Auto,
                detected_game_source: None,
            };

            // Skip texture validation for BSP-converted OBJs (textures may not exist)
            if let Err(e) = perform_conversion(&opts, true) {
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
        let match_brickadia_colorset = self.match_brickadia_colorset;
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
                missing_resources_dialog: None,
                pending_conversion_skip_textures: false,
                logger: logger.clone(),
                conversion_in_progress: true,
                conversion_done_receiver: None,
                conversion_progress: progress.clone(),
                conversion_stage: stage.clone(),
                conversion_cancelled: cancelled.clone(),
                // BSP fields (not used in OBJ conversion path)
                input_file_type: InputFileType::Obj,
                bsp_game_source: GameSource::Auto,
                detected_game_source: None,
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
    
    if opts.split_by_material {
        // Load models and materials once
        set_progress(opts, 5, "Loading models and materials...");
        opts.logger.log("Loading models and materials...".to_string());
        let (mut models, material_images) = load_models_and_materials(opts, skip_textures)?;
        let material_count = material_images.len();

        if material_count == 0 {
            opts.logger.log("No materials found, falling back to single grid".to_string());
            set_progress(opts, 20, "Voxelizing...");
            let mut octree = voxelize_models(&mut models, &material_images, opts, None);
            set_progress(opts, 70, "Writing BRZ file...");
            return write_brz_data(&mut octree, opts, None);
        }

        opts.logger.log(format!("Found {} materials, processing each separately", material_count));

        // Process each material separately
        let mut material_grids: Vec<(Entity, Vec<Brick>)> = Vec::new();

        for mat_id in 0..material_count {
            if is_cancelled(opts) {
                opts.logger.log("Conversion cancelled.".to_string());
                return Ok(());
            }
            
            let base_progress = 10 + (mat_id * 80 / material_count) as u32;
            set_progress(opts, base_progress, &format!("Processing material {} of {}", mat_id + 1, material_count));
            opts.logger.log(format!("Processing material {} of {}", mat_id + 1, material_count));

            // Voxelize only this material
            let mut octree = voxelize_models(&mut models, &material_images, opts, Some(mat_id));

            let max_merge = 500;
            let mut save_data = SaveData {
                bricks: Vec::new(),
                colors: palette::DEFAULT_PALETTE.to_vec(),
                author_name: opts.save_owner_name.clone(),
            };

            set_progress(opts, base_progress + 5, &format!("Simplifying material {}...", mat_id + 1));
            opts.logger.log(format!("Simplifying material {}...", mat_id));
            if opts.simplify {
                simplify_lossy(&mut octree, &mut save_data, opts, max_merge);
            } else {
                simplify_lossless(&mut octree, &mut save_data, opts, max_merge);
            }

            if !save_data.bricks.is_empty() {
                opts.logger.log(format!("Material {} generated {} bricks", mat_id, save_data.bricks.len()));

                // Create a frozen grid entity for this material with user-defined offset
                let offset_multiplier = mat_id as f32;
                let entity = Entity {
                    frozen: true,
                    location: brdb::Vector3f {
                        x: opts.grid_offset_x * offset_multiplier,
                        y: opts.grid_offset_y * offset_multiplier,
                        z: opts.grid_offset_z * offset_multiplier,
                    },
                    ..Default::default()
                };

                material_grids.push((entity, save_data.bricks));
            } else {
                opts.logger.log(format!("Material {} had no bricks, skipping", mat_id));
            }
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
) -> ConversionResult<(Vec<tobj::Model>, Vec<image::RgbaImage>)> {
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

    // Debug: Log model statistics
    debug_log(&opt.logger, format!("Loaded {} model(s)", models.len()));
    for (i, model) in models.iter().enumerate() {
        let vertex_count = model.mesh.positions.len() / 3;
        let face_count = model.mesh.indices.len() / 3;
        debug_log(&opt.logger, format!("  Model {}: '{}' - {} vertices, {} faces", 
            i, model.name, vertex_count, face_count));
    }

    opt.logger.log("Loading materials...".to_string());
    let mut material_images = Vec::<image::RgbaImage>::new();

    let materials = materials.unwrap_or_else(|_| Vec::new());

    if materials.is_empty() {
        opt.logger.log("  No materials found, using default white color".to_string());
        material_images.push(create_solid_color_texture([1.0, 1.0, 1.0], 1.0));
    } else {
        for material in materials {
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

                    opt.logger.log(format!(
                        "  Loading diffuse texture for {} from: {:?}",
                        material.name, image_path
                    ));

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
                    // No texture or empty texture name
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
                opt.logger.log(format!(
                    "  Skipping textures for material {}, using material color",
                    material.name
                ));
                let diffuse = material.diffuse.unwrap_or([1.0, 1.0, 1.0]);
                let dissolve = material.dissolve.unwrap_or(1.0);
                material_images.push(create_solid_color_texture(diffuse, dissolve));
            }
        }
    }

    // Scale models
    scale_models(&mut models, opt.scale, opt.bricktype);

    Ok((models, material_images))
}

fn scale_models(models: &mut [tobj::Model], scale: f32, bricktype: BrickType) {
    // Determine model AABB to expand triangle octree to final size
    // Multiply y-coordinate by 2.5 to take into account plates
    let yscale = if bricktype == BrickType::Microbricks { 1.0 } else { 2.5 };

    for m in models.iter_mut() {
        let p = &mut m.mesh.positions;
        for v in (0..p.len()).step_by(3) {
            p[v] *= scale;
            p[v + 1] *= yscale * scale;
            p[v + 2] *= scale;
        }
    }

    // Raise mesh so no vertices are vertically negative
    if let Some(first_model) = models.first() {
        let positions = &first_model.mesh.positions;
        if !positions.is_empty() {
            let mut min_z = positions[2];
            for m in models.iter() {
                let p = &m.mesh.positions;
                for v in (0..p.len()).step_by(3) {
                    min_z = min_z.min(p[v + 2]);
                }
            }

            if min_z < 0.0 {
                let z_offset = -min_z;
                for m in models.iter_mut() {
                    let p = &mut m.mesh.positions;
                    for v in (0..p.len()).step_by(3) {
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
    let result = voxelize_with_progress(models, material_images, opts.scale, opts.bricktype, material_filter, Some(progress.clone()));
    let elapsed = start.elapsed();
    
    // Stop heartbeat thread
    heartbeat_running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = heartbeat_handle.join();
    
    let final_voxels = progress.triangles_processed.load(std::sync::atomic::Ordering::Relaxed);
    let voxels_fmt = format_number(final_voxels);
    let rate = if elapsed.as_secs() > 0 { final_voxels / elapsed.as_secs() as usize } else { final_voxels };
    let rate_fmt = format_number(rate);
    opts.logger.log(format!("Voxelization completed: {} voxels in {:.2?} (~{}/s avg)", voxels_fmt, elapsed, rate_fmt));
    
    result
}

fn generate_octree(opt: &Obj2Brs, skip_textures: bool, material_filter: Option<usize>) -> ConversionResult<octree::VoxelTree<Vector4<u8>>> {
    opt.logger.log(format!("Loading {:?}", Path::new(&opt.input_file_path)));
    let (mut models, material_images) = load_models_and_materials(opt, skip_textures)?;
    Ok(voxelize_models(&mut models, &material_images, opt, material_filter))
}

fn write_brz_data(octree: &mut octree::VoxelTree<Vector4<u8>>, opts: &Obj2Brs, material_id: Option<usize>) -> ConversionResult<()> {
    let max_merge = 500;

    let mut save_data = SaveData {
        bricks: Vec::new(),
        colors: palette::DEFAULT_PALETTE.to_vec(),
        author_name: opts.save_owner_name.clone(),
    };

    set_progress(opts, 60, "Simplifying...");
    if let Some(id) = material_id {
        opts.logger.log(format!("Simplifying material {}...", id));
    } else {
        opts.logger.log("Simplifying...".to_string());
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
    debug_log(&opts.logger, format!("Generated {} bricks", save_data.bricks.len()));
    debug_log(&opts.logger, format!("Using {} colors", save_data.colors.len()));

    // Write file
    set_progress(opts, 85, &format!("Writing {} bricks...", save_data.bricks.len()));
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

    opts.logger.log(format!("Save written to: {:?}", output_file_path));
    Ok(())
}

fn write_brz_with_grids(opts: &Obj2Brs, grids: Vec<(Entity, Vec<Brick>)>) -> ConversionResult<()> {
    opts.logger.log(format!("Writing {} frozen grids...", grids.len()));

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

    opts.logger.log(format!("Save written to: {:?}", output_file_path));
    Ok(())
}

fn main() {
    let logger = Logger::new();
    logger.log("obj2brz started - ready to convert OBJ files to Brickadia saves".to_string());

    // Log the data directory location
    let data_dir = logger::get_data_dir();
    let cache_dir = logger::get_cache_dir();
    logger.log(format!("Data directory: {:?}", data_dir));
    logger.log(format!("User cache: {:?}", cache_dir));

    let build_dir = match env::consts::OS {
        "windows" => {
            dirs::data_local_dir()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .map(|s| s + "\\Brickadia\\Saved\\Builds")
                .unwrap_or_else(|| "builds".to_string())
        }
        "linux" => {
            dirs::config_dir()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .map(|s| s + "/Epic/Brickadia/Saved/Builds")
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
