//! UI rendering and interaction code for the application.
//!
//! Contains the `App` trait implementation and all UI panel rendering methods.

use bsp_converter::{GameSource, is_bsp_file, detect_game_source};
use eframe::{egui, egui::*, App};
use rfd::FileDialog;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use uuid::Uuid;

use super::gui;
use crate::conversion::material_mapping;
use crate::util::open_folder_in_explorer;

use super::types::*;

pub const WINDOW_WIDTH: f32 = 600.;
pub const WINDOW_HEIGHT: f32 = 700.;

impl App for Obj2Brz {
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

impl Obj2Brz {
    /// Update detected file type and game source based on input path.
    pub fn update_input_file_type(&mut self) {
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

    pub fn receive_file_dialog_messages(&mut self) {
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

    pub fn paths(&mut self, ui: &mut Ui, input_file_valid: bool, output_dir_valid: bool) {
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

        ui.label("Description")
            .on_hover_text("Description text embedded in the .brz file.\nVisible in Brickadia when viewing save info.");
        ui.add(TextEdit::singleline(&mut self.save_description).desired_width(400.0));
        ui.end_row();
    }

    pub fn options(&mut self, ui: &mut Ui, _uuid_valid: bool) {
        let lossy_help = "Merges adjacent bricks of similar colors to reduce brick count.\n\n\
            This significantly reduces file size but may lose fine detail.\n\
            Recommended for large models.\n\n\
            WARNING: This is computationally expensive and can take 5-10+ minutes for complex models.";
        ui.horizontal(|ui| {
            ui.label("Lossy Conversion");
            gui::help_button(ui, "help_lossy", lossy_help, &mut self.help_lossy_expanded);
        });
        ui.horizontal(|ui| {
            ui.add(Checkbox::new(&mut self.simplify, "Simplify (reduces brickcount)"));
            ui.label(RichText::new("(Expensive)").small().color(egui::Color32::from_rgb(255, 180, 0)));
        });
        ui.end_row();
        
        if self.help_lossy_expanded {
            ui.label("");
            egui::Frame::none()
                .fill(egui::Color32::from_gray(30))
                .inner_margin(8.0)
                .rounding(4.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(lossy_help).color(egui::Color32::LIGHT_GRAY));
                });
            ui.end_row();
        }

        ui.label("Brick Surface").on_hover_text(
            "Choose between studded or smooth brick surfaces.\n\n\
            Studded: Traditional LEGO-style bricks with visible studs on top.\n\
            Smooth: Flat surfaces without studs for a polished, clean finish.\n\n\
            Smooth bricks are ideal for architectural models and clean surfaces.");
        ui.add(Checkbox::new(&mut self.use_smooth_bricks, "Use smooth bricks (no studs)"));
        ui.end_row();

        let color_merge_help = "Merge adjacent bricks with similar colors to reduce brick count.\n\n\
            Uses Delta E color difference for perceptual accuracy:\n\
            - Off: No merging\n\
            - Noticeable (1-2): Perceptible under close scrutiny\n\
            - Obvious (2-10): Visible at a glance\n\
            - Distinct (11-49): Clearly different, still similar\n\
            - Very Different (50-70): Major hue/lightness shifts\n\n\
            Only merges touching bricks within the same material group.";
        ui.horizontal(|ui| {
            ui.label("Color Merge");
            gui::help_button(ui, "help_color_merge", color_merge_help, &mut self.help_color_merge_expanded);
        });
        
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
        
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_source("color_merge_combo")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.color_merge_threshold, 0.0, "Off");
                    ui.selectable_value(&mut self.color_merge_threshold, 1.5, "Noticeable (1-2)");
                    ui.selectable_value(&mut self.color_merge_threshold, 6.0, "Obvious (2-10)");
                    ui.selectable_value(&mut self.color_merge_threshold, 30.0, "Distinct (11-49)");
                    ui.selectable_value(&mut self.color_merge_threshold, 60.0, "Very Different (50-70)");
                });
            if self.color_merge_threshold > 0.0 {
                ui.label(RichText::new("(Adds processing time)").small().color(egui::Color32::from_rgb(255, 180, 0)));
            }
        });
        ui.end_row();
        
        if self.help_color_merge_expanded {
            ui.label("");
            egui::Frame::none()
                .fill(egui::Color32::from_gray(30))
                .inner_margin(8.0)
                .rounding(4.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(color_merge_help).color(egui::Color32::LIGHT_GRAY));
                });
            ui.end_row();
        }

        ui.label("Scale")
            .on_hover_text("Multiplier for the final build size in Brickadia.\n\n\
            x1.0 = 1 unit in OBJ equals 1 Brickadia unit.\n\
            x2.0 = Build will be twice as large.\n\
            x0.5 = Build will be half the size.\n\
            x0.125 = 1/8 scale (very small).\n\n\
            Range: 0.01 to 100.0");
        ui.horizontal(|ui| {
            ui.add(
                DragValue::new(&mut self.scale)
                    .min_decimals(2)
                    .prefix("x")
                    .speed(0.01)
                    .range(0.01..=100.0),
            );
            if self.scale > 5.0 {
                ui.label(RichText::new("(High values = more bricks)").small().color(egui::Color32::from_rgb(255, 180, 0)));
            }
        });
        ui.end_row();

        ui.label("Bricktype")
            .on_hover_text("The brick type used to build the model:\n\n\
            - Microbricks: Smallest bricks (2x2x2 studs). Best detail.\n\
            - Default: Standard bricks with visible studs.\n\
            - Tiles: Flat smooth bricks without studs.");
        ComboBox::from_id_source("bricktype_combo")
            .selected_text(format!("{:?}", &mut self.bricktype))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.bricktype, BrickType::Microbricks, "Microbricks");
                ui.selectable_value(&mut self.bricktype, BrickType::Default, "Default");
                ui.selectable_value(&mut self.bricktype, BrickType::Tiles, "Tiles");
            });
        ui.end_row();

        ui.label("Color Mode").on_hover_text("How colors are applied to bricks:\n\n\
            - Texture Colors: Use colors from model textures (default, most accurate).\n\
            - Model Diffuse Color: Use the OBJ material's diffuse color (Kd from .mtl file).\n\
            - Single Color: Use one solid color for all bricks.");
        ComboBox::from_id_source("color_mode_combo")
            .selected_text(match self.color_mode {
                ColorMode::TextureColors => "Texture Colors",
                ColorMode::MaterialColor => "Model Diffuse Color",
                ColorMode::SingleColor => "Single Color",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.color_mode, ColorMode::TextureColors, "Texture Colors");
                ui.selectable_value(&mut self.color_mode, ColorMode::MaterialColor, "Model Diffuse Color");
                ui.selectable_value(&mut self.color_mode, ColorMode::SingleColor, "Single Color");
            });
        ui.end_row();

        // Show color picker when Single Color mode is selected
        if self.color_mode == ColorMode::SingleColor {
            ui.label("Solid Color").on_hover_text("The color to use for all bricks in Single Color mode.");
            ui.horizontal(|ui| {
                let mut color_srgb: [u8; 3] = [
                    (self.single_color[0] * 255.0) as u8,
                    (self.single_color[1] * 255.0) as u8,
                    (self.single_color[2] * 255.0) as u8,
                ];
                if ui.color_edit_button_srgb(&mut color_srgb).changed() {
                    self.single_color = [
                        color_srgb[0] as f32 / 255.0,
                        color_srgb[1] as f32 / 255.0,
                        color_srgb[2] as f32 / 255.0,
                    ];
                }
                ui.label(format!(
                    "#{:02X}{:02X}{:02X}",
                    color_srgb[0],
                    color_srgb[1],
                    color_srgb[2]
                ));
            });
            ui.end_row();
        }

        // Show info text when Material Color mode is selected
        if self.color_mode == ColorMode::MaterialColor {
            ui.label("");
            ui.label(RichText::new(
                "Colors will be read from the OBJ material file (.mtl).\n\
                Each material's diffuse color (Kd) will be used.\n\
                Materials without colors default to white."
            ).small().color(egui::Color32::GRAY));
            ui.end_row();
        }

        ui.label("Brick Surface").on_hover_text("The Brickadia surface type applied to all bricks:\n\n\
            - Plastic: Standard opaque surface.\n\
            - Glass: Transparent, see-through.\n\
            - Glow: Emits light.\n\
            - Metallic: Shiny reflective surface.\n\
            - Hologram: Translucent with glow effect.\n\
            - Ghost: Semi-transparent, no collision.");
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
                            - 0 = No rotation\n\
                            - 90 = Tilt forward (top faces you)\n\
                            - 180 = Flip upside down\n\
                            - 270 = Tilt backward (bottom faces you)\n\n\
                            Use this to fix models that appear tilted forward/back.",
                        );
                        ComboBox::from_id_source("rotation_x")
                            .selected_text(format!("{}", self.rotation_x))
                            .show_ui(ui, |ui: &mut Ui| {
                                ui.selectable_value(&mut self.rotation_x, 0, "0");
                                ui.selectable_value(&mut self.rotation_x, 90, "90");
                                ui.selectable_value(&mut self.rotation_x, 180, "180");
                                ui.selectable_value(&mut self.rotation_x, 270, "270");
                            });
                        ui.end_row();

                        ui.label("Rotation Y").on_hover_text(
                            "Rotate model around the Y axis (forward-back axis).\n\n\
                            In Brickadia coordinates:\n\
                            - 0 = No rotation\n\
                            - 90 = Roll left (left side up)\n\
                            - 180 = Flip left-right\n\
                            - 270 = Roll right (right side up)\n\n\
                            Use this to fix models that appear rolled/tilted sideways.",
                        );
                        ComboBox::from_id_source("rotation_y")
                            .selected_text(format!("{}", self.rotation_y))
                            .show_ui(ui, |ui: &mut Ui| {
                                ui.selectable_value(&mut self.rotation_y, 0, "0");
                                ui.selectable_value(&mut self.rotation_y, 90, "90");
                                ui.selectable_value(&mut self.rotation_y, 180, "180");
                                ui.selectable_value(&mut self.rotation_y, 270, "270");
                            });
                        ui.end_row();

                        ui.label("Rotation Z").on_hover_text(
                            "Rotate model around the Z axis (up-down axis).\n\n\
                            In Brickadia coordinates:\n\
                            - 0 = No rotation\n\
                            - 90 = Spin 90 counter-clockwise (viewed from above)\n\
                            - 180 = Face opposite direction\n\
                            - 270 = Spin 90 clockwise (viewed from above)\n\n\
                            Use this to change which direction the model faces.",
                        );
                        ComboBox::from_id_source("rotation_z")
                            .selected_text(format!("{}", self.rotation_z))
                            .show_ui(ui, |ui: &mut Ui| {
                                ui.selectable_value(&mut self.rotation_z, 0, "0");
                                ui.selectable_value(&mut self.rotation_z, 90, "90");
                                ui.selectable_value(&mut self.rotation_z, 180, "180");
                                ui.selectable_value(&mut self.rotation_z, 270, "270");
                            });
                        ui.end_row();

                        ui.label("Origin Marker").on_hover_text(
                            "Add color-coded axis markers at the origin (0,0,0) for alignment testing.\n\n\
                            In Brickadia coordinates:\n\
                            - White brick at origin center\n\
                            - Red bricks along +X axis (right in Brickadia)\n\
                            - Green bricks along +Y axis (forward in Brickadia)\n\
                            - Blue bricks along +Z axis (up in Brickadia)\n\n\
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

    pub fn profiles_ui(&mut self, ui: &mut Ui) {
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

    pub fn reset_to_defaults(&mut self) {
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

    pub fn save_as_profile(&mut self, name: String) {
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

    pub fn load_profile(&mut self, index: usize) {
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

    pub fn advanced_options(&mut self, ui: &mut Ui, uuid_valid: bool) {
        ui.label("Surface Intensity").on_hover_text(
            "Controls the strength of the Brickadia surface effect (0-10).\n\n\
            Higher values = stronger glow, more reflective metallic, etc.\n\
            Only affects Glass, Glow, Metallic, Hologram, and Ghost surfaces.",
        );
        ui.add(Slider::new(
            &mut self.material_intensity,
            std::ops::RangeInclusive::new(0, 10),
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
            CollapsingHeader::new("Texture-to-Surface Mapping")
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(5.);
                    
                    ui.label(RichText::new("WARNING: Processing large models with many materials can take 1+ hours")
                        .color(egui::Color32::from_rgb(255, 180, 0)));
                    
                    ui.add_space(10.);
                    
                    gui::add_grid(ui, "experimental_material_grid", |ui| {
                        // Auto-assign Materials option - available regardless of split mode
                        let game_source = self.detected_game_source.unwrap_or(self.bsp_game_source);
                        let has_config = game_source != GameSource::Auto && 
                            material_mapping::MaterialMapping::find_config_path(game_source).is_some();
                        
                        ui.label("Auto-assign Surfaces").on_hover_text(
                            "Map textures to Brickadia surfaces based on texture names.\n\n\
                            Uses YAML config files in data/game_textures/<engine>/<game>/auto_2block_materials.yaml\n\
                            to determine which textures should be Glass, Metallic, Glow, etc.\n\n\
                            Example: GLASS* textures = Glass surface, METAL* = Metallic\n\n\
                            Each surface can have its own intensity setting in the config.\n\n\
                            Works in both single-grid and split-by-texture modes.\n\
                            When enabled, the global Brick Surface and Surface Intensity settings are ignored.",
                        );
                        
                        ui.horizontal(|ui| {
                            if has_config {
                                ui.add(Checkbox::new(&mut self.use_material_mapping, "Enable"));
                                ui.label(RichText::new("(Adds processing time)").small().color(egui::Color32::from_rgb(255, 180, 0)));
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

                        ui.label("Split by Texture Group").on_hover_text(
                            "Creates separate frozen brick grids for each OBJ texture group.\n\n\
                            Useful for models with distinct parts you want to move independently.\n\
                            Each texture group becomes its own selectable group in Brickadia.\n\n\
                            When disabled with Auto-assign Surfaces enabled, bricks are still\n\
                            assigned per-texture surfaces but output to a single grid.",
                        );
                        ui.horizontal(|ui| {
                            ui.add(Checkbox::new(&mut self.split_by_material, "Separate grids per texture group"));
                            ui.label(RichText::new("(Increases file size)").small().color(egui::Color32::from_rgb(255, 180, 0)));
                        });
                        ui.end_row();

                        // Additional options when split_by_material is enabled
                        if self.split_by_material {
                            // Group by Brickadia material type option
                            ui.label("Group by Surface Type").on_hover_text(
                                "Consolidate output grids by Brickadia surface type.\n\n\
                                OFF: One grid per texture (300+ grids for complex maps)\n\
                                ON: One grid per surface type (~6 grids: Plastic, Glass, Glow, Metallic, etc.)\n\n\
                                Fewer grids = simpler to manage, but less granular selection in-game.",
                            );
                            ui.add(Checkbox::new(&mut self.group_by_brick_material, "Consolidate grids"));
                            ui.end_row();

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

        let id_color = gui::bool_color(uuid_valid);

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

    pub fn cache_options(&mut self, ui: &mut Ui) {
        let cache_dir = super::logger::get_cache_dir();
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
                if let Err(e) = super::logger::flush_cache() {
                    self.logger.log(format!("Failed to clear cache: {}", e));
                } else {
                    self.logger.log("Cache cleared. Settings will reset on next launch.".to_string());
                }
            }
            ui.label(RichText::new("(Requires restart to take effect)").small().color(egui::Color32::GRAY));
        });

        ui.add_space(5.);
        let data_dir = super::logger::get_data_dir();
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
            let prefabs_path = match std::env::consts::OS {
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

    pub fn show_missing_resources_dialog(&mut self, ctx: &egui::Context) {
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
                        ui.label("- Yes: Use solid colors from material definitions");
                        ui.label("- No: Cancel conversion so you can fix the missing textures");

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
}
