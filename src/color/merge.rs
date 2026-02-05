use brdb::{Brick, Color};
#[allow(unused_imports)]
use brdb::BrickType;
use crate::app::SaveData;
use std::collections::HashMap;
use rayon::prelude::*;

/// Pre-computed brick data for fast color comparison
struct BrickColorData {
    idx: usize,
    lab: (f32, f32, f32),
    color: Color,
    position: (i32, i32, i32),
    size: (i32, i32, i32),
}

/// Merge adjacent bricks with similar colors to reduce brick count.
/// Uses Delta E 2000 (CIEDE2000) for perceptual color difference.
/// Optimized with pre-computed LAB colors and parallel processing.
pub fn merge_similar_colors(save_data: &mut SaveData, threshold: f32, logger: &crate::app::Logger) {
    if threshold <= 0.0 {
        return;
    }

    let original_count = save_data.bricks.len();
    logger.log(format!("Starting optimized color merge with threshold {:.1} on {} bricks...", threshold, original_count));

    // Phase 1: Pre-compute LAB colors (expensive operation done once)
    logger.log("Phase 1: Pre-computing LAB colors...".to_string());
    
    // First collect basic data (sequential, since Brick may not be Send)
    let brick_basics: Vec<_> = save_data.bricks.iter()
        .enumerate()
        .map(|(idx, brick)| {
            (idx, brick.color, (brick.position.x, brick.position.y, brick.position.z), get_brick_size(brick))
        })
        .collect();
    
    // Then compute LAB in parallel (just the color conversion)
    let brick_data: Vec<BrickColorData> = brick_basics.par_iter()
        .map(|(idx, color, position, size)| {
            BrickColorData {
                idx: *idx,
                lab: rgb_to_lab(color.r, color.g, color.b),
                color: *color,
                position: *position,
                size: *size,
            }
        })
        .collect();

    // Phase 2: Group by color buckets (quantized LAB) for faster matching
    logger.log("Phase 2: Grouping bricks by color similarity...".to_string());
    
    // Quantize LAB to buckets (bucket size based on threshold)
    let bucket_size = (threshold * 2.0).max(5.0);
    let mut color_buckets: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
    
    for data in &brick_data {
        let bucket_key = (
            (data.lab.0 / bucket_size) as i32,
            (data.lab.1 / bucket_size) as i32,
            (data.lab.2 / bucket_size) as i32,
        );
        color_buckets.entry(bucket_key).or_default().push(data.idx);
    }
    
    logger.log(format!("Created {} color buckets", color_buckets.len()));

    // Phase 3: Build spatial index
    logger.log("Phase 3: Building spatial index...".to_string());
    let mut spatial_map: HashMap<(i32, i32, i32), usize> = HashMap::new();
    for data in &brick_data {
        spatial_map.insert(data.position, data.idx);
    }
    
    // Debug: Comprehensive logging for diagnosis
    logger.log("[DEBUG] === SPATIAL ANALYSIS ===".to_string());
    
    // Analyze brick sizes
    let mut size_counts: HashMap<(i32, i32, i32), usize> = HashMap::new();
    for data in &brick_data {
        *size_counts.entry(data.size).or_default() += 1;
    }
    logger.log(format!("[DEBUG] Unique brick sizes: {}", size_counts.len()));
    for (size, count) in size_counts.iter().take(5) {
        logger.log(format!("[DEBUG]   Size ({},{},{}): {} bricks", size.0, size.1, size.2, count));
    }
    
    // Sample some brick positions
    if brick_data.len() >= 3 {
        for i in 0..3 {
            let sample = &brick_data[i];
            logger.log(format!("[DEBUG] Brick {}: pos=({},{},{}), size=({},{},{})", 
                i, sample.position.0, sample.position.1, sample.position.2,
                sample.size.0, sample.size.1, sample.size.2));
        }
    }
    
    // Check neighbor connectivity
    let mut total_neighbors_found = 0;
    let mut bricks_with_neighbors = 0;
    let mut color_matches = 0;
    let mut color_mismatches = 0;
    
    for data in &brick_data {
        let neighbors = [
            (data.position.0 + 2 * data.size.0, data.position.1, data.position.2),
            (data.position.0 - 2 * data.size.0, data.position.1, data.position.2),
            (data.position.0, data.position.1 + 2 * data.size.1, data.position.2),
            (data.position.0, data.position.1 - 2 * data.size.1, data.position.2),
            (data.position.0, data.position.1, data.position.2 + 2 * data.size.2),
            (data.position.0, data.position.1, data.position.2 - 2 * data.size.2),
        ];
        
        let mut has_neighbor = false;
        for neighbor_pos in neighbors {
            if let Some(&neighbor_idx) = spatial_map.get(&neighbor_pos) {
                has_neighbor = true;
                total_neighbors_found += 1;
                
                let neighbor_data = &brick_data[neighbor_idx];
                let delta_e = calculate_delta_e_lab(data.lab, neighbor_data.lab);
                if delta_e <= threshold {
                    color_matches += 1;
                } else {
                    color_mismatches += 1;
                }
            }
        }
        if has_neighbor {
            bricks_with_neighbors += 1;
        }
    }
    
    logger.log(format!("[DEBUG] Bricks with neighbors: {}/{} ({:.1}%)", 
        bricks_with_neighbors, brick_data.len(), 
        100.0 * bricks_with_neighbors as f32 / brick_data.len() as f32));
    logger.log(format!("[DEBUG] Total neighbor pairs found: {}", total_neighbors_found));
    logger.log(format!("[DEBUG] Color matches (delta_e <= {}): {}", threshold, color_matches));
    logger.log(format!("[DEBUG] Color mismatches: {}", color_mismatches));
    logger.log("[DEBUG] === END SPATIAL ANALYSIS ===".to_string());

    // Phase 4: Find merge groups using Union-Find (much faster than flood-fill)
    logger.log("Phase 4: Finding merge groups with Union-Find...".to_string());
    
    // Initialize Union-Find structure
    let n = brick_data.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank: Vec<usize> = vec![0; n];
    
    // Find with path compression
    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }
    
    // Union by rank
    fn union(parent: &mut [usize], rank: &mut [usize], x: usize, y: usize) {
        let root_x = find(parent, x);
        let root_y = find(parent, y);
        if root_x != root_y {
            if rank[root_x] < rank[root_y] {
                parent[root_x] = root_y;
            } else if rank[root_x] > rank[root_y] {
                parent[root_y] = root_x;
            } else {
                parent[root_y] = root_x;
                rank[root_x] += 1;
            }
        }
    }
    
    // Single pass: check each brick's neighbors and union if colors match
    let mut unions_performed = 0;
    for data in &brick_data {
        let neighbors = [
            (data.position.0 + 2 * data.size.0, data.position.1, data.position.2),
            (data.position.0 - 2 * data.size.0, data.position.1, data.position.2),
            (data.position.0, data.position.1 + 2 * data.size.1, data.position.2),
            (data.position.0, data.position.1 - 2 * data.size.1, data.position.2),
            (data.position.0, data.position.1, data.position.2 + 2 * data.size.2),
            (data.position.0, data.position.1, data.position.2 - 2 * data.size.2),
        ];
        
        for neighbor_pos in neighbors {
            if let Some(&neighbor_idx) = spatial_map.get(&neighbor_pos) {
                if neighbor_idx > data.idx {
                    let neighbor_data = &brick_data[neighbor_idx];
                    let delta_e = calculate_delta_e_lab(data.lab, neighbor_data.lab);
                    
                    if delta_e <= threshold {
                        union(&mut parent, &mut rank, data.idx, neighbor_idx);
                        unions_performed += 1;
                    }
                }
            }
        }
    }
    
    logger.log(format!("[DEBUG] Union operations performed: {}", unions_performed));
    
    // Collect groups by root
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    
    // Analyze group sizes
    let single_brick_groups = groups.values().filter(|g| g.len() == 1).count();
    let multi_brick_groups = groups.len() - single_brick_groups;
    let largest_group = groups.values().map(|g| g.len()).max().unwrap_or(0);
    let avg_group_size = if multi_brick_groups > 0 {
        groups.values().filter(|g| g.len() > 1).map(|g| g.len()).sum::<usize>() as f32 / multi_brick_groups as f32
    } else {
        0.0
    };
    
    logger.log(format!("Found {} merge groups ({} single, {} multi-brick)", 
        groups.len(), single_brick_groups, multi_brick_groups));
    logger.log(format!("[DEBUG] Largest group: {} bricks, avg multi-brick group: {:.1} bricks", 
        largest_group, avg_group_size));
    
    // Process groups - unify colors within each group (keep all bricks, just change colors)
    // NOTE: We do NOT remove bricks or expand bounding boxes because that destroys
    // the original shape (e.g., an L-shaped region would become a filled rectangle)
    logger.log("Phase 5: Unifying colors within merge groups...".to_string());
    
    let mut color_changes: HashMap<usize, Color> = HashMap::new();
    let mut bricks_recolored = 0;
    
    for (_root, group) in &groups {
        if group.len() > 1 {
            // Find the most common color in the group
            let most_common_color = find_most_common_color_fast(&brick_data, group);
            
            // Apply this color to all bricks in the group
            for &idx in group {
                if brick_data[idx].color != most_common_color {
                    color_changes.insert(idx, most_common_color);
                    bricks_recolored += 1;
                }
            }
        }
    }
    
    logger.log(format!("Recoloring {} bricks across {} multi-brick groups", 
        bricks_recolored, multi_brick_groups));

    // Apply color changes
    logger.log("Phase 6: Applying color unification...".to_string());
    for (&idx, &new_color) in &color_changes {
        if idx < save_data.bricks.len() {
            save_data.bricks[idx].color = new_color;
        }
    }
    
    // No bricks are removed - we just unified colors
    logger.log("Phase 7: Color unification complete (no bricks removed)".to_string());

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

/// Get brick size from asset type
fn get_brick_size(brick: &Brick) -> (i32, i32, i32) {
    use brdb::BrickType;
    match &brick.asset {
        BrickType::Procedural { size, .. } => (size.x as i32, size.y as i32, size.z as i32),
        _ => (5, 5, 2),
    }
}

/// Fast Delta E calculation using pre-computed LAB values
#[inline]
fn calculate_delta_e_lab(lab1: (f32, f32, f32), lab2: (f32, f32, f32)) -> f32 {
    let dl = lab1.0 - lab2.0;
    let da = lab1.1 - lab2.1;
    let db = lab1.2 - lab2.2;
    (dl * dl + da * da + db * db).sqrt()
}

/// Find most common color using pre-computed data
fn find_most_common_color_fast(brick_data: &[BrickColorData], group: &[usize]) -> Color {
    let mut color_counts: HashMap<(u8, u8, u8), usize> = HashMap::new();
    
    for &idx in group {
        if idx < brick_data.len() {
            let color = brick_data[idx].color;
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

/// Remove unused colors from the palette after merging
fn cleanup_palette(save_data: &mut SaveData, logger: &crate::app::Logger) {
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
