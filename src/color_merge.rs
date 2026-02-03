use brdb::{Brick, Color};
use crate::SaveData;
use std::collections::HashMap;

/// Merge adjacent bricks with similar colors to reduce brick count.
/// Uses Delta E 2000 (CIEDE2000) for perceptual color difference.
pub fn merge_similar_colors(save_data: &mut SaveData, threshold: f32, logger: &crate::logger::Logger) {
    if threshold <= 0.0 {
        return;
    }

    let original_count = save_data.bricks.len();
    logger.log(format!("Starting color merge with threshold {:.1} on {} bricks...", threshold, original_count));

    // Group bricks by material to respect material boundaries
    let mut material_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, brick) in save_data.bricks.iter().enumerate() {
        material_groups.entry(brick.material.to_string())
            .or_insert_with(Vec::new)
            .push(idx);
    }

    logger.log(format!("Grouped into {} material types", material_groups.len()));

    // Process each material group separately
    let mut bricks_to_remove = Vec::new();
    let mut color_replacements: HashMap<usize, Color> = HashMap::new();

    for (_material, indices) in material_groups.iter() {
        if indices.len() < 2 {
            continue;
        }

        // Build spatial index for fast neighbor lookups
        let mut spatial_map: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for &idx in indices {
            let brick = &save_data.bricks[idx];
            let key = (brick.position.x, brick.position.y, brick.position.z);
            spatial_map.entry(key).or_insert_with(Vec::new).push(idx);
        }

        // Find groups of touching bricks with similar colors
        let mut visited = vec![false; save_data.bricks.len()];
        
        for &idx in indices {
            if visited[idx] {
                continue;
            }

            let brick = &save_data.bricks[idx];
            let base_color = brick.color;
            
            // Find all touching bricks with similar colors
            let mut group = vec![idx];
            let mut to_check = vec![idx];
            visited[idx] = true;

            while let Some(current_idx) = to_check.pop() {
                let current_brick = &save_data.bricks[current_idx];
                
                // Check all 6 adjacent positions (touching faces)
                let neighbors = get_touching_positions(&current_brick);
                
                for neighbor_pos in neighbors {
                    if let Some(neighbor_indices) = spatial_map.get(&neighbor_pos) {
                        for &neighbor_idx in neighbor_indices {
                            if visited[neighbor_idx] {
                                continue;
                            }

                            let neighbor_brick = &save_data.bricks[neighbor_idx];
                            
                            // Check if colors are similar using Delta E
                            let delta_e = calculate_delta_e(base_color, neighbor_brick.color);
                            
                            // Threshold is now a direct Delta E value from the UI dropdown
                            if delta_e <= threshold {
                                visited[neighbor_idx] = true;
                                group.push(neighbor_idx);
                                to_check.push(neighbor_idx);
                            }
                        }
                    }
                }
            }

            // If we found a group of 2+ bricks, merge them
            if group.len() > 1 {
                // Find the most common color in the group
                let most_common_color = find_most_common_color(&save_data.bricks, &group);
                
                // Keep the first brick, mark others for removal
                for &brick_idx in &group[1..] {
                    bricks_to_remove.push(brick_idx);
                }
                
                // Replace all colors in the group with the most common one
                for &brick_idx in &group {
                    color_replacements.insert(brick_idx, most_common_color);
                }
            }
        }
    }

    // Apply color replacements
    for (idx, new_color) in color_replacements {
        if idx < save_data.bricks.len() {
            save_data.bricks[idx].color = new_color;
        }
    }

    // Remove merged bricks (in reverse order to maintain indices)
    bricks_to_remove.sort_unstable();
    bricks_to_remove.dedup();
    for &idx in bricks_to_remove.iter().rev() {
        if idx < save_data.bricks.len() {
            save_data.bricks.remove(idx);
        }
    }

    let final_count = save_data.bricks.len();
    let removed = original_count - final_count;
    let percent = if original_count > 0 {
        (removed as f32 / original_count as f32) * 100.0
    } else {
        0.0
    };

    logger.log(format!(
        "Color merge complete: {} bricks removed ({:.1}%), {} bricks remaining",
        removed, percent, final_count
    ));

    // Clean up the color palette - remove unused colors
    cleanup_palette(save_data, logger);
}

/// Remove unused colors from the palette after merging
fn cleanup_palette(save_data: &mut SaveData, logger: &crate::logger::Logger) {
    let original_palette_size = save_data.colors.len();
    
    // Collect all unique colors actually used by bricks
    let mut used_colors = std::collections::HashSet::new();
    for brick in &save_data.bricks {
        let color_key = (brick.color.r, brick.color.g, brick.color.b);
        used_colors.insert(color_key);
    }
    
    // Rebuild palette with only used colors
    let new_palette: Vec<Color> = used_colors
        .into_iter()
        .map(|(r, g, b)| Color::new(r, g, b))
        .collect();
    
    let new_palette_size = new_palette.len();
    save_data.colors = new_palette;
    
    let removed_colors = original_palette_size.saturating_sub(new_palette_size);
    if removed_colors > 0 {
        logger.log(format!(
            "Palette cleanup: removed {} unused colors ({} -> {} colors)",
            removed_colors, original_palette_size, new_palette_size
        ));
    }
}

/// Get the 6 adjacent positions (touching faces) for a brick
fn get_touching_positions(brick: &Brick) -> Vec<(i32, i32, i32)> {
    use brdb::BrickType;
    
    // Extract brick size from the asset
    let size = match &brick.asset {
        BrickType::Procedural { size, .. } => (size.x as i32, size.y as i32, size.z as i32),
        _ => (5, 5, 2), // Default brick size for non-procedural bricks
    };
    
    let pos = &brick.position;
    
    vec![
        (pos.x + size.0, pos.y, pos.z),
        (pos.x - size.0, pos.y, pos.z),
        (pos.x, pos.y + size.1, pos.z),
        (pos.x, pos.y - size.1, pos.z),
        (pos.x, pos.y, pos.z + size.2),
        (pos.x, pos.y, pos.z - size.2),
    ]
}

/// Find the most common color in a group of bricks
fn find_most_common_color(bricks: &[Brick], group: &[usize]) -> Color {
    let mut color_counts: HashMap<(u8, u8, u8), usize> = HashMap::new();
    
    for &idx in group {
        if idx < bricks.len() {
            let color = bricks[idx].color;
            let key = (color.r, color.g, color.b);
            *color_counts.entry(key).or_insert(0) += 1;
        }
    }
    
    let most_common = color_counts.iter()
        .max_by_key(|(_, count)| *count)
        .map(|(color, _)| *color)
        .unwrap_or((255, 255, 255));
    
    Color::new(most_common.0, most_common.1, most_common.2)
}

/// Calculate Delta E color difference (simplified version)
fn calculate_delta_e(color1: Color, color2: Color) -> f32 {
    let lab1 = rgb_to_lab(color1.r, color1.g, color1.b);
    let lab2 = rgb_to_lab(color2.r, color2.g, color2.b);
    
    let dl = lab1.0 - lab2.0;
    let da = lab1.1 - lab2.1;
    let db = lab1.2 - lab2.2;
    
    (dl * dl + da * da + db * db).sqrt()
}

/// Convert RGB to LAB color space
fn rgb_to_lab(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    
    let r = if r > 0.04045 { ((r + 0.055) / 1.055).powf(2.4) } else { r / 12.92 };
    let g = if g > 0.04045 { ((g + 0.055) / 1.055).powf(2.4) } else { g / 12.92 };
    let b = if b > 0.04045 { ((b + 0.055) / 1.055).powf(2.4) } else { b / 12.92 };
    
    let x = r * 0.4124564 + g * 0.3575761 + b * 0.1804375;
    let y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;
    let z = r * 0.0193339 + g * 0.1191920 + b * 0.9503041;
    
    let x = x / 0.95047;
    let y = y / 1.00000;
    let z = z / 1.08883;
    
    let fx = if x > 0.008856 { x.powf(1.0 / 3.0) } else { (7.787 * x) + (16.0 / 116.0) };
    let fy = if y > 0.008856 { y.powf(1.0 / 3.0) } else { (7.787 * y) + (16.0 / 116.0) };
    let fz = if z > 0.008856 { z.powf(1.0 / 3.0) } else { (7.787 * z) + (16.0 / 116.0) };
    
    let l = (116.0 * fy) - 16.0;
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);
    
    (l, a, b)
}
