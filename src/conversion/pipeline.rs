//! Main conversion pipeline orchestration.
//!
//! This module contains the core conversion logic that transforms OBJ models
//! into Brickadia BRZ files. It coordinates model loading, voxelization,
//! simplification, and BRZ writing.

use bsp_converter::GameSource;
use brdb::{Brick, Entity};
use cgmath::Vector4;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread;

use crate::app::{BrickType, Material, Obj2Brz, SaveData, load_preview_icon};
use crate::output::brdb_support;
use crate::color;
use super::brick_alignment::{align_bricks_to_world, BrickAlignmentConfig};
use super::grid_assembly::{assemble_grids, GridAssemblyConfig, MaterialBricks};
use super::material_processing::MaterialBatchStats;
use super::material_mapping;
use crate::error::{ConversionError, ConversionResult};
use crate::voxel::{self as octree, voxelize_with_progress, voxelize_from_pregrouped, PreGroupedTriangles, VoxelizeProgress};
use crate::simplify::{simplify_lossy, simplify_lossless, simplify_lossy_with_material, simplify_lossless_with_material};
use crate::util::{create_solid_color_texture, debug_log, format_number, DEBUG_MODE, VERBOSE_MODEL_LOADING, VERBOSE_TEXTURE_LOADING};

pub fn set_progress(opts: &Obj2Brz, percent: u32, stage: &str) {
    opts.conversion_progress.store(percent, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut s) = opts.conversion_stage.lock() {
        *s = stage.to_string();
    }
}

/// Check if the conversion has been cancelled
pub fn is_cancelled(opts: &Obj2Brz) -> bool {
    opts.conversion_cancelled.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn perform_conversion(opts: &Obj2Brz, skip_textures: bool) -> ConversionResult<()> {
    let start_time = std::time::Instant::now();
    
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
    opts.logger.log(format!("Rotation: X={}deg, Y={}deg, Z={}deg", opts.rotation_x, opts.rotation_y, opts.rotation_z));
    opts.logger.log(format!("Show Origin Marker: {}", opts.show_origin_marker));
    opts.logger.log(format!("Skip Textures: {}", skip_textures));
    
    // Log experimental material processing settings
    opts.logger.log(format!("Auto-assign Materials: {}", opts.use_material_mapping));
    opts.logger.log(format!("Split by Material: {}", opts.split_by_material));
    if opts.split_by_material || opts.use_material_mapping {
        opts.logger.log(format!("  - Group by Brick Material: {}", opts.group_by_brick_material || !opts.split_by_material));
        if opts.split_by_material {
            opts.logger.log(format!("  - Grid Offset: X={}, Y={}, Z={}", opts.grid_offset_x, opts.grid_offset_y, opts.grid_offset_z));
        }
    }
    opts.logger.log("===========================".to_string());
    opts.logger.log("".to_string());
    
    // Use per-material processing if either split_by_material OR use_material_mapping is enabled
    // When only use_material_mapping is enabled (not split_by_material), we process per-material
    // but consolidate into a single grid by forcing group_by_brick_material behavior
    let use_per_material_processing = opts.split_by_material || opts.use_material_mapping;
    
    let result = if use_per_material_processing {
        perform_per_material_conversion(opts, skip_textures)
    } else {
        // Check if we should use per-object processing to reduce memory
        // This is automatically enabled when the octree would be too large
        set_progress(opts, 10, "Loading model...");
        let (models, material_images, _material_names) = load_models_and_materials(opts, skip_textures)?;
        
        // Check if per-object processing would be beneficial
        let should_use_per_object = should_use_per_object_processing(&models, opts);
        
        if should_use_per_object {
            perform_per_object_conversion(models, material_images, opts)
        } else {
            // Regular single-grid conversion
            let mut octree = voxelize_models(&mut models.clone(), &material_images, opts, None);
            
            if is_cancelled(opts) {
                opts.logger.log("Conversion cancelled.".to_string());
                return Ok(());
            }
            
            set_progress(opts, 50, "Simplifying and generating bricks...");
            write_brz_data(&mut octree, opts, None)
        }
    };
    
    // Log total conversion time
    let elapsed = start_time.elapsed();
    let minutes = elapsed.as_secs() / 60;
    let seconds = elapsed.as_secs() % 60;
    if minutes > 0 {
        opts.logger.log(format!("=== CONVERSION COMPLETE: {}m {}s ===", minutes, seconds));
    } else {
        opts.logger.log(format!("=== CONVERSION COMPLETE: {:.1}s ===", elapsed.as_secs_f32()));
    }
    
    result
}

fn perform_per_object_conversion(
    models: Vec<tobj::Model>,
    material_images: Vec<image::RgbaImage>,
    opts: &Obj2Brz,
) -> ConversionResult<()> {
    use rayon::prelude::*;
    use crate::voxel::voxelize;
    
    let object_count = models.len();
    opts.logger.log(format!("Processing {} objects with parallel voxelization...", object_count));
    
    // Phase 1: Parallel voxelization (thread-safe)
    // Translate each object to local space for small octrees
    set_progress(opts, 10, "Phase 1: Parallel voxelization...");
    opts.logger.log("Phase 1: Voxelizing all objects in parallel (local space)...".to_string());
    
    let material_images_ref = &material_images;
    let scale = opts.scale;
    let bricktype = opts.bricktype;
    
    // Voxelize each object in local coordinates, track world offset
    let voxelized_objects: Vec<_> = models.par_iter()
        .filter(|model| !model.mesh.positions.is_empty())
        .map(|model| {
            // Calculate object bounds in world space (before scaling)
            let positions = &model.mesh.positions;
            let mut min_x = positions[0];
            let mut min_y = positions[1];
            let mut min_z = positions[2];
            
            for v in (0..positions.len()).step_by(3) {
                min_x = min_x.min(positions[v]);
                min_y = min_y.min(positions[v + 1]);
                min_z = min_z.min(positions[v + 2]);
            }
            
            // Create local-space copy (translate to origin)
            let mut local_model = model.clone();
            for v in (0..local_model.mesh.positions.len()).step_by(3) {
                local_model.mesh.positions[v] -= min_x;
                local_model.mesh.positions[v + 1] -= min_y;
                local_model.mesh.positions[v + 2] -= min_z;
            }
            
            // Voxelize in local space (thread-safe, no logger)
            let single_model = vec![local_model];
            let octree = voxelize(&single_model, material_images_ref, scale, bricktype, None);
            
            // Return octree with world offset (in original model units, before scaling)
            (octree, min_x, min_y, min_z)
        })
        .collect();
    
    opts.logger.log(format!("Phase 1 complete: {} objects voxelized", voxelized_objects.len()));
    
    // Phase 2: Sequential simplification (requires Obj2Brz)
    set_progress(opts, 50, "Phase 2: Simplifying bricks...");
    opts.logger.log("Phase 2: Simplifying and generating bricks (sequential)...".to_string());
    
    let mut save_data = SaveData {
        bricks: Vec::new(),
        colors: color::palette::DEFAULT_PALETTE.to_vec(),
        author_name: opts.save_owner_name.clone(),
    };
    
    let total_objects = voxelized_objects.len();
    for (obj_idx, (mut octree, min_x, min_y, min_z)) in voxelized_objects.into_iter().enumerate() {
        if is_cancelled(opts) {
            opts.logger.log("Conversion cancelled.".to_string());
            return Ok(());
        }
        
        // Update progress
        let progress_pct = 50 + ((obj_idx as u32 * 40) / total_objects as u32);
        set_progress(opts, progress_pct, &format!("Simplifying object {}/{}", obj_idx + 1, total_objects));
        
        // Track brick count before this object
        let bricks_before = save_data.bricks.len();
        
        // Simplify this object's octree
        let grid_size = 1u64 << (octree.size + 1);
        let grid_volume = grid_size.pow(3);
        let bytes_per_voxel = std::mem::size_of::<Option<cgmath::Vector4<u8>>>() as u64;
        let estimated_bytes = grid_volume.saturating_mul(bytes_per_voxel);
        let estimated_gb = estimated_bytes as f64 / 1_073_741_824.0;
        
        if estimated_gb <= 16.0 && opts.simplify {
            let max_merge = 500;
            if opts.color_merge_threshold > 0.0 {
                simplify_lossy(&mut octree, &mut save_data, opts, max_merge);
            } else {
                simplify_lossless(&mut octree, &mut save_data, opts, max_merge);
            }
        } else {
            crate::simplify::generate_bricks_direct(&octree, &mut save_data, opts);
        }
        
        // Translate bricks back to world space
        // The voxelizer processes scaled model coordinates, and create_brick handles the Y/Z swap internally
        // We need to apply the same transformation to the offset as create_brick does to positions
        // create_brick: brick_pos = (2 * scale * x, 2 * scale * z, 2 * scale * y)
        // So offset should be: (2 * brick_scale * min_x, 2 * brick_scale * min_z, 2 * brick_scale * min_y)
        let brick_scale = opts.brick_scale as f32;
        
        // Calculate offset - use floor to ensure we don't get negative positions from rounding
        let world_offset_x = (2.0 * brick_scale * min_x).floor() as i32;
        let world_offset_y = (2.0 * brick_scale * min_z).floor() as i32;
        let world_offset_z = (2.0 * brick_scale * min_y).floor() as i32;
        
        // Apply offset and ensure no negative positions
        for brick in &mut save_data.bricks[bricks_before..] {
            brick.position.x += world_offset_x;
            brick.position.y += world_offset_y;
            brick.position.z += world_offset_z;
            
            // Clamp to non-negative (safety check)
            brick.position.x = brick.position.x.max(0);
            brick.position.y = brick.position.y.max(0);
            brick.position.z = brick.position.z.max(0);
        }
        
        // Log progress every 500 objects
        if (obj_idx + 1) % 500 == 0 {
            opts.logger.log(format!("  Converted {}/{} objects ({} bricks so far)", 
                obj_idx + 1, total_objects, save_data.bricks.len()));
        }
    }
    
    opts.logger.log(format!("All {} objects processed, {} total bricks generated", 
        total_objects, save_data.bricks.len()));
    
    // Validate brick positions before writing
    let mut min_pos = (i32::MAX, i32::MAX, i32::MAX);
    let mut max_pos = (i32::MIN, i32::MIN, i32::MIN);
    let mut negative_count = 0;
    for brick in &save_data.bricks {
        min_pos.0 = min_pos.0.min(brick.position.x);
        min_pos.1 = min_pos.1.min(brick.position.y);
        min_pos.2 = min_pos.2.min(brick.position.z);
        max_pos.0 = max_pos.0.max(brick.position.x);
        max_pos.1 = max_pos.1.max(brick.position.y);
        max_pos.2 = max_pos.2.max(brick.position.z);
        if brick.position.x < 0 || brick.position.y < 0 || brick.position.z < 0 {
            negative_count += 1;
        }
    }
    opts.logger.log(format!("[DEBUG] Brick position range: ({},{},{}) to ({},{},{})", 
        min_pos.0, min_pos.1, min_pos.2, max_pos.0, max_pos.1, max_pos.2));
    if negative_count > 0 {
        opts.logger.log(format!("[WARNING] {} bricks have negative positions!", negative_count));
    }
    
    // Apply color merge if requested
    if opts.color_merge_threshold > 0.0 {
        set_progress(opts, 90, "Merging similar colors...");
        color::merge::merge_similar_colors(&mut save_data, opts.color_merge_threshold, &opts.logger);
    }
    
    // Write final BRZ file
    set_progress(opts, 95, "Writing BRZ file...");
    write_final_brz(&save_data, opts)?;
    
    set_progress(opts, 100, "Complete!");
    Ok(())
}

fn perform_per_material_conversion(opts: &Obj2Brz, skip_textures: bool) -> ConversionResult<()> {
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
    
    // Collect material bricks for grid assembly using the new module
    let mut all_material_bricks: Vec<MaterialBricks> = Vec::new();
    
    // Grid assembly configuration from user options
    // When use_material_mapping is enabled but split_by_material is disabled,
    // force consolidation into a single grid by grouping all bricks together
    let force_single_grid = opts.use_material_mapping && !opts.split_by_material;
    let grid_config = GridAssemblyConfig {
        grid_offset_x: if force_single_grid { 0.0 } else { opts.grid_offset_x },
        grid_offset_y: if force_single_grid { 0.0 } else { opts.grid_offset_y },
        grid_offset_z: if force_single_grid { 0.0 } else { opts.grid_offset_z },
        // Force grouping when material mapping is on but split is off - this consolidates into a single grid
        group_by_brick_material: opts.group_by_brick_material || force_single_grid,
    };

    // Track processing statistics using the material_processing module
    let mut stats = MaterialBatchStats::default();
    
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
            stats.record_skipped();
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
                stats.record_skipped();
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
        
        // Calculate ETA based on average processing time from stats
        let materials_remaining = material_count - mat_id - 1;
        let avg_time = stats.average_time_per_material();
        let eta_seconds = (materials_remaining as f32 * avg_time).max(0.0);
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
        // Returns (octree, world_offset) where world_offset = material's bounds.min
        // 
        // IMPORTANT: We pass None for global_offset to use per-material bounds for octree sizing.
        // This keeps octrees small and efficient (size 3-6 instead of 9-10).
        // 
        // World alignment is handled by adding world_offset back to brick positions below.
        // Each material's bricks are placed at their correct world position because
        // world_offset equals the material's bounds.min.
        let (mut octree, world_offset) = voxelize_from_pregrouped(&pregrouped, &material_images, mat_id, None, None, Some(&opts.logger));
        
        // Log voxelization results
        debug_log(&opts.logger, format!("Material {} ({}) voxelized: octree size {}, offset({:.1},{:.1},{:.1})", 
            mat_id, mat_name, octree.size, world_offset.x, world_offset.y, world_offset.z));

        let max_merge = 500;
        let mut save_data = SaveData {
            bricks: Vec::new(),
            colors: color::palette::DEFAULT_PALETTE.to_vec(),
            author_name: opts.save_owner_name.clone(),
        };

        if opts.simplify {
            simplify_lossy_with_material(&mut octree, &mut save_data, opts, max_merge, brick_material, brick_intensity);
        } else {
            simplify_lossless_with_material(&mut octree, &mut save_data, opts, max_merge, brick_material, brick_intensity);
        }
        
        // Apply world offset to restore original positions using the brick_alignment module
        // This handles: offset application, position range tracking, and negative position correction
        let alignment_config = BrickAlignmentConfig {
            brick_scale_factor: 2.0 * opts.brick_scale as f32,
            global_reference: None,
        };
        let alignment_result = align_bricks_to_world(&mut save_data.bricks, world_offset, &alignment_config);
        
        if DEBUG_MODE && !save_data.bricks.is_empty() {
            let range = &alignment_result.position_range;
            debug_log(&opts.logger, format!("Brick positions after alignment: X[{},{}] Y[{},{}] Z[{},{}]",
                range.min_x, range.max_x, range.min_y, range.max_y, range.min_z, range.max_z));
            if alignment_result.corrections_applied {
                debug_log(&opts.logger, "Auto-corrected negative positions".to_string());
            }
        }

        let mat_elapsed = mat_start.elapsed();
        let elapsed_secs = mat_elapsed.as_secs_f32();
        
        if !save_data.bricks.is_empty() {
            // Record processed material stats
            stats.record_processed(save_data.bricks.len(), octree.size, elapsed_secs);
            
            opts.logger.log(format!("Material {} ({}, {} tris) -> {} bricks as {:?} in {:.2}s", 
                mat_id, mat_name, triangle_count, save_data.bricks.len(), brick_material, elapsed_secs));

            // Collect bricks for grid assembly (handled by grid_assembly module)
            all_material_bricks.push(MaterialBricks {
                material_id: mat_id,
                material_name: mat_name.clone(),
                bricks: save_data.bricks,
                brick_material,
            });
        } else {
            debug_log(&opts.logger, format!("Material {} ({}) -> 0 bricks in {:.2}s (skipped)", 
                mat_id, mat_name, elapsed_secs));
        }
    }

    // Assemble grids using the grid_assembly module
    // This handles both per-texture-material and group-by-brick-material modes
    let assembly_result = assemble_grids(all_material_bricks, &grid_config);
    let mut material_grids = assembly_result.grids;
    
    if force_single_grid {
        opts.logger.log(format!("Auto-assign Materials: Consolidated {} texture materials into {} grid(s) with per-texture Brickadia materials", 
            stats.processed_count, assembly_result.grid_count));
    } else if grid_config.group_by_brick_material {
        opts.logger.log(format!("Consolidated {} texture materials into {} grids by Brickadia material type", 
            stats.processed_count, assembly_result.grid_count));
    }

    // Log summary with stats from material_processing module
    opts.logger.log(format!("Processed {} materials -> {} grids, {} total bricks ({} skipped)", 
        stats.processed_count, assembly_result.grid_count, stats.total_bricks, stats.skipped_count));
    
    if stats.large_octree_count > 0 {
        opts.logger.log(format!("Warning: {} materials had octree size >= 9 (slow processing)", 
            stats.large_octree_count));
    }

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
}

pub fn load_models_and_materials(
    opt: &Obj2Brz,
    skip_textures: bool,
) -> ConversionResult<(Vec<tobj::Model>, Vec<image::RgbaImage>, Vec<String>)> {
    use crate::app::ColorMode;
    
    let p = Path::new(&opt.input_file_path);

    opt.logger.log("Importing model...".to_string());
    debug_log(&opt.logger, format!("Input file: {:?}", p));
    debug_log(&opt.logger, format!("Skip textures: {}", skip_textures));
    debug_log(&opt.logger, format!("Color mode: {:?}", opt.color_mode));
    
    let load_options = tobj::LoadOptions {
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

    // Handle SingleColor mode - use the user-selected color for all materials
    if opt.color_mode == ColorMode::SingleColor {
        opt.logger.log(format!(
            "  Using Single Color mode: RGB({:.2}, {:.2}, {:.2})",
            opt.single_color[0], opt.single_color[1], opt.single_color[2]
        ));
        
        if materials.is_empty() {
            material_images.push(create_solid_color_texture(opt.single_color, 1.0));
            material_names.push("single_color".to_string());
        } else {
            for material in materials {
                material_names.push(material.name.clone());
                let dissolve = material.dissolve.unwrap_or(1.0);
                material_images.push(create_solid_color_texture(opt.single_color, dissolve));
            }
        }
    }
    // Handle MaterialColor mode - use OBJ material diffuse colors, skip textures
    else if opt.color_mode == ColorMode::MaterialColor {
        opt.logger.log("  Using Material Color mode: colors from OBJ material diffuse (Kd)".to_string());
        
        if materials.is_empty() {
            opt.logger.log("  No materials found, using default white color".to_string());
            material_images.push(create_solid_color_texture([1.0, 1.0, 1.0], 1.0));
            material_names.push("default".to_string());
        } else {
            for material in materials {
                material_names.push(material.name.clone());
                let diffuse = material.diffuse.unwrap_or([1.0, 1.0, 1.0]);
                let dissolve = material.dissolve.unwrap_or(1.0);
                if VERBOSE_TEXTURE_LOADING {
                    opt.logger.log(format!(
                        "  Material {}: diffuse RGB({:.2}, {:.2}, {:.2})",
                        material.name, diffuse[0], diffuse[1], diffuse[2]
                    ));
                }
                material_images.push(create_solid_color_texture(diffuse, dissolve));
            }
        }
    }
    // TextureColors mode (default) - load textures when available
    else if materials.is_empty() {
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

    // CRITICAL: Eliminate negative coordinates FIRST, before any transformations
    // This ensures the entire pipeline only works with non-negative world space positions
    eliminate_negative_coordinates(&mut models, &opt.logger);
    
    // Check for large coordinate ranges and warn user
    check_model_bounds(&models, opt);

    // Apply rotation if any rotation is set
    if opt.rotation_x != 0 || opt.rotation_y != 0 || opt.rotation_z != 0 {
        rotate_models(&mut models, opt.rotation_x, opt.rotation_y, opt.rotation_z);
        opt.logger.log(format!(
            "Applied rotation: X={}deg, Y={}deg, Z={}deg",
            opt.rotation_x, opt.rotation_y, opt.rotation_z
        ));
    }

    // Scale models (centers at origin, then eliminates negatives again)
    scale_models(&mut models, opt.scale, opt.scale_x, opt.scale_y, opt.scale_z, &opt.logger);

    Ok((models, material_images, material_names))
}

fn eliminate_negative_coordinates(models: &mut [tobj::Model], logger: &crate::app::Logger) {
    if models.is_empty() {
        return;
    }
    
    let first_model = &models[0];
    let positions = &first_model.mesh.positions;
    if positions.is_empty() {
        return;
    }
    
    // Find minimum coordinates across all models
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
            "Eliminating negative coordinates: offsetting by X+{:.1}, Y+{:.1}, Z+{:.1}",
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

fn check_model_bounds(models: &[tobj::Model], opt: &Obj2Brz) {
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

fn scale_models(models: &mut [tobj::Model], scale: f32, scale_x: f32, scale_y: f32, scale_z: f32, logger: &crate::app::Logger) {
    // Apply base scale plus per-axis scale multipliers.
    // Note: scale_x/y/z are in Brickadia coordinates, but OBJ uses Y-up.
    // Since simplify swaps Y<->Z, we need to swap scale_y and scale_z here.
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

fn should_use_per_object_processing(models: &[tobj::Model], opts: &Obj2Brz) -> bool {
    if models.is_empty() {
        return false;
    }
    
    // Calculate the octree size for the entire model
    let positions = &models[0].mesh.positions;
    if positions.is_empty() {
        return false;
    }
    
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
    
    let size_x = (max_x - min_x).ceil() as isize;
    let size_y = (max_y - min_y).ceil() as isize;
    let size_z = (max_z - min_z).ceil() as isize;
    let max_size = size_x.max(size_y).max(size_z);
    
    let mut octree_size = 0u8;
    while (1isize << octree_size) < max_size {
        octree_size += 1;
    }
    
    let grid_size = 1u64 << (octree_size + 1);
    let grid_volume = grid_size.pow(3);
    let bytes_per_voxel = std::mem::size_of::<Option<cgmath::Vector4<u8>>>() as u64;
    let estimated_bytes = grid_volume.saturating_mul(bytes_per_voxel);
    let estimated_gb = estimated_bytes as f64 / 1_073_741_824.0;
    
    // TEMPORARILY DISABLED: Use per-object processing if octree would be > 16GB and we have multiple objects
    // This path has issues that need to be fixed
    if false && estimated_gb > 16.0 && models.len() > 1 {
        opts.logger.log(format!(
            "Detected large model ({:.1} GB octree) with {} objects.",
            estimated_gb, models.len()
        ));
        opts.logger.log("Using per-object octree processing to enable simplification...".to_string());
        return true;
    }
    
    false
}

fn predict_and_warn_octree_size(models: &[tobj::Model], opts: &Obj2Brz) {
    if models.is_empty() {
        return;
    }
    
    let positions = &models[0].mesh.positions;
    if positions.is_empty() {
        return;
    }
    
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
    
    let size_x = (max_x - min_x).ceil() as isize;
    let size_y = (max_y - min_y).ceil() as isize;
    let size_z = (max_z - min_z).ceil() as isize;
    let max_size = size_x.max(size_y).max(size_z);
    
    let mut octree_size = 0u8;
    while (1isize << octree_size) < max_size {
        octree_size += 1;
    }
    
    let grid_size = 1u64 << (octree_size + 1);
    let grid_volume = grid_size.pow(3);
    let bytes_per_voxel = std::mem::size_of::<Option<cgmath::Vector4<u8>>>() as u64;
    let estimated_bytes = grid_volume.saturating_mul(bytes_per_voxel);
    let estimated_gb = estimated_bytes as f64 / 1_073_741_824.0;
    
    if estimated_gb > 16.0 {
        opts.logger.log(format!(
            "WARNING: Current scale ({}) will create a {}^3 octree requiring {:.1} GB of memory.",
            opts.scale, grid_size, estimated_gb
        ));
        opts.logger.log("Simplification will be skipped. Color merge and brick optimization may not work as expected.".to_string());
        
        let recommended_scale = opts.scale * (estimated_gb / 8.0).powf(1.0 / 3.0) as f32;
        opts.logger.log(format!(
            "SUGGESTION 1: Increase scale to {:.2} or higher to enable full simplification features.",
            recommended_scale
        ));
        opts.logger.log("SUGGESTION 2: Enable 'Split by Material' to process materials separately (reduces memory per material).".to_string());
    }
}

fn voxelize_models(
    models: &mut [tobj::Model],
    material_images: &[image::RgbaImage],
    opts: &Obj2Brz,
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
    
    // Predict octree size and warn if it will be too large
    if material_filter.is_none() {
        predict_and_warn_octree_size(models, opts);
    }
    
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
    debug_log(&opts.logger, format!("Octree size: {} (grid dimensions: {}^3)", result.size, grid_size));
    
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

fn generate_octree(opt: &Obj2Brz, skip_textures: bool, material_filter: Option<usize>) -> ConversionResult<octree::VoxelTree<Vector4<u8>>> {
    opt.logger.log(format!("Loading {}", Path::new(&opt.input_file_path).display()));
    let (mut models, material_images, _material_names) = load_models_and_materials(opt, skip_textures)?;
    Ok(voxelize_models(&mut models, &material_images, opt, material_filter))
}

fn write_brz_data(octree: &mut octree::VoxelTree<Vector4<u8>>, opts: &Obj2Brz, material_id: Option<usize>) -> ConversionResult<()> {
    let max_merge = 500;

    let mut save_data = SaveData {
        bricks: Vec::new(),
        colors: color::palette::DEFAULT_PALETTE.to_vec(),
        author_name: opts.save_owner_name.clone(),
    };

    // Check if octree is too large for simplification (would cause memory overflow)
    // VoxelGrid allocates size^3 where size = 2^(octree.size+1)
    // We need to ensure size^3 * sizeof(Option<Vector4<u8>>) < reasonable memory limit
    let grid_size = 1u64 << (octree.size + 1);
    let grid_volume = grid_size.pow(3);
    let bytes_per_voxel = std::mem::size_of::<Option<Vector4<u8>>>() as u64;
    let estimated_bytes = grid_volume.saturating_mul(bytes_per_voxel);
    let estimated_gb = estimated_bytes as f64 / 1_073_741_824.0;
    
    debug_log(&opts.logger, format!("Simplification memory estimate: {:.2} GB for {}^3 grid", estimated_gb, grid_size));
    
    // If estimated memory > 16 GB, skip simplification and use direct brick generation
    if estimated_gb > 16.0 {
        opts.logger.log(format!(
            "WARNING: Model is too large for simplification ({:.1} GB required).",
            estimated_gb
        ));
        opts.logger.log("Generating bricks directly from octree (1 brick per voxel).".to_string());
        opts.logger.log("Tip: Use a larger scale value to reduce brick count.".to_string());
        
        // Generate bricks directly without VoxelGrid allocation
        crate::simplify::generate_bricks_direct(octree, &mut save_data, opts);
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
        color::merge::merge_similar_colors(&mut save_data, opts.color_merge_threshold, &opts.logger);
    }

    set_progress(opts, 95, "Writing file...");
    opts.logger.log(format!("Writing {} bricks...", save_data.bricks.len()));

    let preview_bytes = load_preview_icon();
    let preview = image::load_from_memory_with_format(&preview_bytes, image::ImageFormat::Png)
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

fn write_final_brz(save_data: &SaveData, opts: &Obj2Brz) -> ConversionResult<()> {
    let preview_bytes = load_preview_icon();
    let preview = image::load_from_memory_with_format(&preview_bytes, image::ImageFormat::Png)
        .map_err(|e| ConversionError::SaveWriteError(format!("Failed to load preview icon: {}", e)))?;

    let mut preview_bytes_jpg = Vec::new();
    preview
        .write_to(&mut Cursor::new(&mut preview_bytes_jpg), image::ImageOutputFormat::Jpeg(85))
        .map_err(|e| ConversionError::SaveWriteError(format!("Failed to encode JPEG preview: {}", e)))?;

    let output_file_path = PathBuf::from(&opts.output_directory).join(opts.save_name.clone() + ".brz");

    let use_procedural = opts.bricktype == BrickType::Microbricks;
    brdb_support::write_brz(
        output_file_path.clone(),
        save_data,
        opts,
        use_procedural,
        Some(preview_bytes_jpg),
    )?;

    opts.logger.log(format!("Successfully wrote BRZ to {}", output_file_path.display()));
    opts.logger.log(format!("Save written to: {}", output_file_path.display()));
    Ok(())
}

fn write_brz_with_grids(opts: &Obj2Brz, mut grids: Vec<(Entity, Vec<Brick>)>) -> ConversionResult<()> {
    opts.logger.log(format!("Writing {} frozen grids...", grids.len()));

    // Apply color merging to each grid if enabled
    if opts.color_merge_threshold > 0.0 {
        set_progress(opts, 92, "Merging similar colors across grids...");
        let grid_count = grids.len();
        for (idx, (_, bricks)) in grids.iter_mut().enumerate() {
            let mut save_data = SaveData {
                bricks: std::mem::take(bricks),
                colors: color::palette::DEFAULT_PALETTE.to_vec(),
                author_name: opts.save_owner_name.clone(),
            };
            color::merge::merge_similar_colors(&mut save_data, opts.color_merge_threshold, &opts.logger);
            *bricks = save_data.bricks;
            
            if (idx + 1) % 10 == 0 {
                opts.logger.log(format!("Processed {}/{} grids for color merging", idx + 1, grid_count));
            }
        }
    }

    let preview_bytes = load_preview_icon();
    let preview = image::load_from_memory_with_format(&preview_bytes, image::ImageFormat::Png)
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
fn add_origin_marker(save_data: &mut SaveData, opts: &Obj2Brz) {
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
fn create_origin_marker_bricks(opts: &Obj2Brz) -> Vec<Brick> {
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
