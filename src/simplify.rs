use crate::color::*;
use crate::logger::Logger;
use crate::octree::{Branches, TreeBody, VoxelTree};
use crate::{BrickType, Material, Obj2Brz, SaveData};

use brdb::{Brick, BrickSize, BrickType as BrdbBrickType, Color, Direction, Position, Rotation};
use cgmath::Vector4;

// Enum to represent brick colors (index or unique)
#[derive(Debug, Clone, Copy)]
pub enum BrickColor {
    Index(u32),
    Unique(Color),
}

/// A flat 3D grid for O(1) voxel access during simplification.
/// This replaces the O(log n) octree traversal with direct array indexing.
struct VoxelGrid {
    /// Flat array of voxels, indexed as [x + y * size + z * size * size]
    data: Vec<Option<Vector4<u8>>>,
    /// Size of the grid in each dimension (grid is size x size x size)
    size: usize,
    /// Offset to convert from octree coordinates (which can be negative) to grid indices
    offset: isize,
}

impl VoxelGrid {
    /// Convert octree to flat grid. The octree spans from -2^size to 2^size in each dimension.
    fn from_octree(octree: &VoxelTree<Vector4<u8>>, logger: &Logger) -> Self {
        let half_size = 1isize << octree.size;
        let size = (half_size * 2) as usize;
        let offset = half_size; // Add this to convert octree coord to grid index
        
        let mut data = vec![None; size * size * size];
        
        // Recursively extract all leaves from octree
        Self::extract_leaves(&octree.contents, half_size, -half_size, -half_size, -half_size, &mut data, size, offset);
        
        // Debug: count extracted voxels and their coordinate ranges
        let mut count = 0usize;
        let mut min_x = isize::MAX;
        let mut max_x = isize::MIN;
        let mut min_y = isize::MAX;
        let mut max_y = isize::MIN;
        let mut min_z = isize::MAX;
        let mut max_z = isize::MIN;
        for idx in 0..data.len() {
            if data[idx].is_some() {
                count += 1;
                let x = (idx % size) as isize - offset;
                let y = ((idx / size) % size) as isize - offset;
                let z = (idx / (size * size)) as isize - offset;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                min_z = min_z.min(z);
                max_z = max_z.max(z);
            }
        }
        if crate::DEBUG_MODE {
            logger.log(format!("[DEBUG] Extracted {} voxels from octree (size {})", count, octree.size));
            logger.log(format!("[DEBUG] Voxel coordinate ranges (local): X[{},{}] Y[{},{}] Z[{},{}]", 
                min_x, max_x, min_y, max_y, min_z, max_z));
            
            // Warn if negative coordinates detected (blocks will be at negative positions)
            if min_x < 0 || min_y < 0 || min_z < 0 {
                logger.log(format!("[WARNING] Negative voxel coordinates detected! Blocks may be placed at negative positions."));
                logger.log(format!("[WARNING] This can cause issues when 'Split by Material' is enabled."));
                logger.log(format!("[WARNING] Consider adjusting Grid Offset settings to ensure positive placement."));
            }
        }
        
        Self { data, size, offset }
    }
    
    fn extract_leaves(
        branches: &Branches<Vector4<u8>>,
        mask: isize,
        base_x: isize,
        base_y: isize,
        base_z: isize,
        data: &mut Vec<Option<Vector4<u8>>>,
        size: usize,
        offset: isize,
    ) {
        let m = mask >> 1;
        let step = 2 * m + ((m == 0) as isize);
        
        for (i, branch) in branches.iter().enumerate() {
            let x = base_x + step * ((i & 4) > 0) as isize;
            let y = base_y + step * ((i & 2) > 0) as isize;
            let z = base_z + step * ((i & 1) > 0) as isize;
            
            match branch {
                TreeBody::Branch(b) => {
                    if m > 0 {
                        Self::extract_leaves(b, m, x, y, z, data, size, offset);
                    }
                }
                TreeBody::Leaf(color) => {
                    if m == 0 {
                        let idx = Self::coord_to_index_static(x, y, z, size, offset);
                        if idx < data.len() {
                            data[idx] = Some(*color);
                        }
                    }
                }
                TreeBody::Empty => {}
            }
        }
    }
    
    #[inline(always)]
    fn coord_to_index_static(x: isize, y: isize, z: isize, size: usize, offset: isize) -> usize {
        let gx = (x + offset) as usize;
        let gy = (y + offset) as usize;
        let gz = (z + offset) as usize;
        gx + gy * size + gz * size * size
    }
    
    #[inline(always)]
    fn coord_to_index(&self, x: isize, y: isize, z: isize) -> usize {
        Self::coord_to_index_static(x, y, z, self.size, self.offset)
    }
    
    #[inline(always)]
    fn get(&self, x: isize, y: isize, z: isize) -> Option<Vector4<u8>> {
        let idx = self.coord_to_index(x, y, z);
        if idx < self.data.len() {
            self.data[idx]
        } else {
            None
        }
    }
    
    #[inline(always)]
    fn clear(&mut self, x: isize, y: isize, z: isize) {
        let idx = self.coord_to_index(x, y, z);
        if idx < self.data.len() {
            self.data[idx] = None;
        }
    }
    
    /// Find the next non-empty voxel starting from the given position.
    /// Returns None if no more voxels exist.
    fn find_next_voxel(&self, start_idx: usize) -> Option<(usize, isize, isize, isize, Vector4<u8>)> {
        for idx in start_idx..self.data.len() {
            if let Some(color) = self.data[idx] {
                let size = self.size;
                let x = (idx % size) as isize - self.offset;
                let y = ((idx / size) % size) as isize - self.offset;
                let z = (idx / (size * size)) as isize - self.offset;
                return Some((idx, x, y, z, color));
            }
        }
        None
    }
}

pub fn simplify_lossy(
    octree: &mut VoxelTree<Vector4<u8>>,
    save_data: &mut SaveData,
    opts: &Obj2Brz,
    max_merge: isize,
) {
    // Convert octree to flat grid for O(1) access
    let mut grid = VoxelGrid::from_octree(octree, &opts.logger);
    
    let colorset = convert_colorset_to_hsv(&save_data.colors);
    let scales: (isize, isize, isize) = if opts.bricktype == BrickType::Microbricks {
        (opts.brick_scale, opts.brick_scale, opts.brick_scale)
    } else {
        (5, 5, 2)
    };

    let max_scale = isize::max(isize::max(scales.0, scales.1), scales.2);
    let max_merge = max_merge / max_scale;
    let grid_len = grid.size as isize;

    let mut search_idx = 0usize;
    
    while let Some((idx, x, y, z, first_color)) = grid.find_next_voxel(search_idx) {
        search_idx = idx; // Start next search from here
        
        let mut colors = vec![first_color];

        let mut xp = x + 1;
        let mut yp = y + 1;
        let mut zp = z + 1;

        // Expand z direction first
        while zp - z < max_merge && zp < grid_len {
            if let Some(color) = grid.get(x, y, zp) {
                colors.push(color);
                zp += 1;
            } else {
                break;
            }
        }

        // Expand y direction
        while yp - y < max_merge && yp < grid_len {
            let mut pass = true;
            for sz in z..zp {
                if grid.get(x, yp, sz).is_none() {
                    pass = false;
                    break;
                }
            }
            if !pass {
                break;
            }
            // Collect colors
            for sz in z..zp {
                if let Some(color) = grid.get(x, yp, sz) {
                    colors.push(color);
                }
            }
            yp += 1;
        }

        // Expand x direction
        while xp - x < max_merge && xp < grid_len {
            let mut pass = true;
            'outer: for sy in y..yp {
                for sz in z..zp {
                    if grid.get(xp, sy, sz).is_none() {
                        pass = false;
                        break 'outer;
                    }
                }
            }
            if !pass {
                break;
            }
            // Collect colors
            for sy in y..yp {
                for sz in z..zp {
                    if let Some(color) = grid.get(xp, sy, sz) {
                        colors.push(color);
                    }
                }
            }
            xp += 1;
        }

        // Clear voxels in grid (O(1) per voxel)
        for sx in x..xp {
            for sy in y..yp {
                for sz in z..zp {
                    grid.clear(sx, sy, sz);
                }
            }
        }

        let avg_color = hsv_average(&colors);
        let color = if opts.match_brickadia_colorset {
            BrickColor::Index(match_hsv_to_colorset(&colorset, &avg_color) as u32)
        } else {
            let rgba = gamma_correct(hsv2rgb(avg_color));
            BrickColor::Unique(Color::new(rgba[0], rgba[1], rgba[2]))
        };

        let width = xp - x;
        let height = yp - y;
        let depth = zp - z;

        // Check if this brick has a top surface (no voxels directly above it)
        // A brick is a top surface if there's no voxel at z=zp for any (x,y) in the brick's footprint
        let is_top_surface = if opts.use_smooth_bricks {
            let mut has_voxel_above = false;
            'top_check: for sx in x..xp {
                for sy in y..yp {
                    if grid.get(sx, sy, zp).is_some() {
                        has_voxel_above = true;
                        break 'top_check;
                    }
                }
            }
            !has_voxel_above
        } else {
            false
        };

        save_data.bricks.push(create_brick_with_surface(
            opts,
            &save_data.colors,
            scales,
            (width, depth, height),
            (x, z, y),
            color,
            is_top_surface,
        ));
    }
}

pub fn simplify_lossless(
    octree: &mut VoxelTree<Vector4<u8>>,
    save_data: &mut SaveData,
    opts: &Obj2Brz,
    max_merge: isize,
) {
    // Convert octree to flat grid for O(1) access
    let mut grid = VoxelGrid::from_octree(octree, &opts.logger);
    
    let colorset = convert_colorset_to_hsv(&save_data.colors);

    let scales: (isize, isize, isize) = if opts.bricktype == BrickType::Microbricks {
        (opts.brick_scale, opts.brick_scale, opts.brick_scale)
    } else {
        (5, 5, 2)
    };

    let max_scale = isize::max(isize::max(scales.0, scales.1), scales.2);
    let max_merge = max_merge / max_scale;
    let grid_len = grid.size as isize;

    let mut search_idx = 0usize;
    
    while let Some((idx, x, y, z, first_color)) = grid.find_next_voxel(search_idx) {
        search_idx = idx; // Start next search from here
        
        let final_color = gamma_correct(first_color);
        let matched_color = match_hsv_to_colorset(&colorset, &rgb2hsv(final_color));
        let unmatched_color = BrickColor::Unique(Color::new(
            final_color[0],
            final_color[1],
            final_color[2],
        ));

        let mut xp = x + 1;
        let mut yp = y + 1;
        let mut zp = z + 1;

        // Expand z direction first
        while zp - z < max_merge && zp < grid_len {
            if let Some(color) = grid.get(x, y, zp) {
                let fc = gamma_correct(color);
                let color_temp = match_hsv_to_colorset(&colorset, &rgb2hsv(fc));
                if color_temp != matched_color {
                    break;
                }
                zp += 1;
            } else {
                break;
            }
        }

        // Expand y direction
        while yp - y < max_merge && yp < grid_len {
            let mut pass = true;
            for sz in z..zp {
                match grid.get(x, yp, sz) {
                    Some(color) => {
                        let fc = gamma_correct(color);
                        let color_temp = match_hsv_to_colorset(&colorset, &rgb2hsv(fc));
                        if color_temp != matched_color {
                            pass = false;
                            break;
                        }
                    }
                    None => {
                        pass = false;
                        break;
                    }
                }
            }
            if !pass {
                break;
            }
            yp += 1;
        }

        // Expand x direction
        while xp - x < max_merge && xp < grid_len {
            let mut pass = true;
            'outer: for sy in y..yp {
                for sz in z..zp {
                    match grid.get(xp, sy, sz) {
                        Some(color) => {
                            let fc = gamma_correct(color);
                            let color_temp = match_hsv_to_colorset(&colorset, &rgb2hsv(fc));
                            if color_temp != matched_color {
                                pass = false;
                                break 'outer;
                            }
                        }
                        None => {
                            pass = false;
                            break 'outer;
                        }
                    }
                }
            }
            if !pass {
                break;
            }
            xp += 1;
        }

        // Clear voxels in grid (O(1) per voxel)
        for sx in x..xp {
            for sy in y..yp {
                for sz in z..zp {
                    grid.clear(sx, sy, sz);
                }
            }
        }

        let width = xp - x;
        let height = yp - y;
        let depth = zp - z;

        let color = if opts.match_brickadia_colorset {
            BrickColor::Index(matched_color as u32)
        } else {
            unmatched_color
        };

        // Check if this brick has a top surface (no voxels directly above it)
        let is_top_surface = if opts.use_smooth_bricks {
            let mut has_voxel_above = false;
            'top_check: for sx in x..xp {
                for sy in y..yp {
                    if grid.get(sx, sy, zp).is_some() {
                        has_voxel_above = true;
                        break 'top_check;
                    }
                }
            }
            !has_voxel_above
        } else {
            false
        };

        save_data.bricks.push(create_brick_with_surface(
            opts,
            &save_data.colors,
            scales,
            (width, depth, height),
            (x, z, y),
            color,
            is_top_surface,
        ));
    }
}

#[allow(dead_code)]
fn create_brick(
    opts: &Obj2Brz,
    palette: &[Color],
    scale: (isize, isize, isize),
    size: (isize, isize, isize),
    pos: (isize, isize, isize),
    color: BrickColor,
) -> Brick {
    // For smooth bricks option, we apply smooth to all bricks in this default path
    // The is_top_surface variant is used when we need selective smoothing
    create_brick_internal(opts, palette, scale, size, pos, color, opts.use_smooth_bricks)
}

/// Create a brick with explicit control over whether it should be smooth.
/// is_top_surface: when true and use_smooth_bricks is enabled, uses smooth tile instead of studded brick
fn create_brick_with_surface(
    opts: &Obj2Brz,
    palette: &[Color],
    scale: (isize, isize, isize),
    size: (isize, isize, isize),
    pos: (isize, isize, isize),
    color: BrickColor,
    is_top_surface: bool,
) -> Brick {
    // Only apply smooth if both the option is enabled AND this is a top surface
    let apply_smooth = opts.use_smooth_bricks && is_top_surface;
    create_brick_internal(opts, palette, scale, size, pos, color, apply_smooth)
}

fn create_brick_internal(
    opts: &Obj2Brz,
    palette: &[Color],
    scale: (isize, isize, isize),
    size: (isize, isize, isize),
    pos: (isize, isize, isize),
    color: BrickColor,
    apply_smooth: bool,
) -> Brick {
    let brick_size = BrickSize::new(
        (scale.0 * size.0) as u16,
        (scale.1 * size.1) as u16,
        (scale.2 * size.2) as u16,
    );

    let position = Position {
        x: (scale.0 * size.0 + 2 * scale.0 * pos.0) as i32,
        y: (scale.1 * size.1 + 2 * scale.1 * pos.1) as i32,
        z: (scale.2 * size.2 + 2 * scale.2 * pos.2) as i32,
    };

    let asset_name = if opts.bricktype == BrickType::Microbricks {
        "PB_DefaultMicroBrick"
    } else if opts.bricktype == BrickType::Tiles || apply_smooth {
        "PB_DefaultSmoothTile"
    } else {
        "PB_DefaultBrick"
    };

    let brick_type = BrdbBrickType::from((asset_name, brick_size));

    let brick_color = match color {
        BrickColor::Unique(c) => c,
        BrickColor::Index(idx) => {
            if (idx as usize) < palette.len() {
                palette[idx as usize]
            } else {
                Color::new(255, 255, 255)
            }
        }
    };

    Brick {
        id: None,
        asset: brick_type,
        owner_index: None,
        position,
        rotation: Rotation::Deg0,
        direction: Direction::ZPositive,
        collision: Default::default(),
        visible: true,
        color: brick_color,
        material: match opts.material {
            crate::Material::Plastic => "BMC_Plastic".into(),
            crate::Material::Glass => "BMC_Glass".into(),
            crate::Material::Glow => "BMC_Glow".into(),
            crate::Material::Metallic => "BMC_Metallic".into(),
            crate::Material::Hologram => "BMC_Hologram".into(),
            crate::Material::Ghost => "BMC_Ghost".into(),
        },
        material_intensity: opts.material_intensity as u8,
        components: Vec::new(),
    }
}

/// Create a brick with a specific material and intensity override (for material mapping).
#[allow(dead_code)]
fn create_brick_with_material(
    opts: &Obj2Brz,
    palette: &[Color],
    scale: (isize, isize, isize),
    size: (isize, isize, isize),
    pos: (isize, isize, isize),
    color: BrickColor,
    material: Material,
    intensity: u8,
) -> Brick {
    create_brick_with_material_internal(opts, palette, scale, size, pos, color, material, intensity, opts.use_smooth_bricks)
}

/// Create a brick with material override and explicit surface control.
fn create_brick_with_material_and_surface(
    opts: &Obj2Brz,
    palette: &[Color],
    scale: (isize, isize, isize),
    size: (isize, isize, isize),
    pos: (isize, isize, isize),
    color: BrickColor,
    material: Material,
    intensity: u8,
    is_top_surface: bool,
) -> Brick {
    let apply_smooth = opts.use_smooth_bricks && is_top_surface;
    create_brick_with_material_internal(opts, palette, scale, size, pos, color, material, intensity, apply_smooth)
}

fn create_brick_with_material_internal(
    opts: &Obj2Brz,
    palette: &[Color],
    scale: (isize, isize, isize),
    size: (isize, isize, isize),
    pos: (isize, isize, isize),
    color: BrickColor,
    material: Material,
    intensity: u8,
    apply_smooth: bool,
) -> Brick {
    let brick_size = BrickSize::new(
        (scale.0 * size.0) as u16,
        (scale.1 * size.1) as u16,
        (scale.2 * size.2) as u16,
    );

    let position = Position {
        x: (scale.0 * size.0 + 2 * scale.0 * pos.0) as i32,
        y: (scale.1 * size.1 + 2 * scale.1 * pos.1) as i32,
        z: (scale.2 * size.2 + 2 * scale.2 * pos.2) as i32,
    };

    let asset_name = if opts.bricktype == BrickType::Microbricks {
        "PB_DefaultMicroBrick"
    } else if opts.bricktype == BrickType::Tiles || apply_smooth {
        "PB_DefaultSmoothTile"
    } else {
        "PB_DefaultBrick"
    };

    let brick_type = BrdbBrickType::from((asset_name, brick_size));

    let brick_color = match color {
        BrickColor::Unique(c) => c,
        BrickColor::Index(idx) => {
            if (idx as usize) < palette.len() {
                palette[idx as usize]
            } else {
                Color::new(255, 255, 255)
            }
        }
    };

    Brick {
        id: None,
        asset: brick_type,
        owner_index: None,
        position,
        rotation: Rotation::Deg0,
        direction: Direction::ZPositive,
        collision: Default::default(),
        visible: true,
        color: brick_color,
        material: match material {
            Material::Plastic => "BMC_Plastic".into(),
            Material::Glass => "BMC_Glass".into(),
            Material::Glow => "BMC_Glow".into(),
            Material::Metallic => "BMC_Metallic".into(),
            Material::Hologram => "BMC_Hologram".into(),
            Material::Ghost => "BMC_Ghost".into(),
        },
        material_intensity: intensity,
        components: Vec::new(),
    }
}

/// Lossy simplification with a specific material and intensity override.
pub fn simplify_lossy_with_material(
    octree: &mut VoxelTree<Vector4<u8>>,
    save_data: &mut SaveData,
    opts: &Obj2Brz,
    max_merge: isize,
    material: Material,
    intensity: u8,
) {
    // Convert octree to flat grid for O(1) access
    let mut grid = VoxelGrid::from_octree(octree, &opts.logger);
    
    let colorset = convert_colorset_to_hsv(&save_data.colors);
    let scales: (isize, isize, isize) = if opts.bricktype == BrickType::Microbricks {
        (opts.brick_scale, opts.brick_scale, opts.brick_scale)
    } else {
        (5, 5, 2)
    };

    let max_scale = isize::max(isize::max(scales.0, scales.1), scales.2);
    let max_merge = max_merge / max_scale;
    let grid_len = grid.size as isize;

    let mut search_idx = 0usize;
    
    while let Some((idx, x, y, z, first_color)) = grid.find_next_voxel(search_idx) {
        search_idx = idx;
        
        let mut colors = vec![first_color];

        let mut xp = x + 1;
        let mut yp = y + 1;
        let mut zp = z + 1;

        // Expand z direction first
        while zp - z < max_merge && zp < grid_len {
            if let Some(color) = grid.get(x, y, zp) {
                colors.push(color);
                zp += 1;
            } else {
                break;
            }
        }

        // Expand y direction
        while yp - y < max_merge && yp < grid_len {
            let mut pass = true;
            for sz in z..zp {
                if grid.get(x, yp, sz).is_none() {
                    pass = false;
                    break;
                }
            }
            if !pass {
                break;
            }
            for sz in z..zp {
                if let Some(color) = grid.get(x, yp, sz) {
                    colors.push(color);
                }
            }
            yp += 1;
        }

        // Expand x direction
        while xp - x < max_merge && xp < grid_len {
            let mut pass = true;
            'outer: for sy in y..yp {
                for sz in z..zp {
                    if grid.get(xp, sy, sz).is_none() {
                        pass = false;
                        break 'outer;
                    }
                }
            }
            if !pass {
                break;
            }
            for sy in y..yp {
                for sz in z..zp {
                    if let Some(color) = grid.get(xp, sy, sz) {
                        colors.push(color);
                    }
                }
            }
            xp += 1;
        }

        // Clear voxels in grid
        for sx in x..xp {
            for sy in y..yp {
                for sz in z..zp {
                    grid.clear(sx, sy, sz);
                }
            }
        }

        let avg_color = hsv_average(&colors);
        let color = if opts.match_brickadia_colorset {
            BrickColor::Index(match_hsv_to_colorset(&colorset, &avg_color) as u32)
        } else {
            let rgba = gamma_correct(hsv2rgb(avg_color));
            BrickColor::Unique(Color::new(rgba[0], rgba[1], rgba[2]))
        };

        let width = xp - x;
        let height = yp - y;
        let depth = zp - z;

        // Check if this brick has a top surface (no voxels directly above it)
        let is_top_surface = if opts.use_smooth_bricks {
            let mut has_voxel_above = false;
            'top_check: for sx in x..xp {
                for sy in y..yp {
                    if grid.get(sx, sy, zp).is_some() {
                        has_voxel_above = true;
                        break 'top_check;
                    }
                }
            }
            !has_voxel_above
        } else {
            false
        };

        save_data.bricks.push(create_brick_with_material_and_surface(
            opts,
            &save_data.colors,
            scales,
            (width, depth, height),
            (x, z, y),
            color,
            material,
            intensity,
            is_top_surface,
        ));
    }
}

/// Lossless simplification with a specific material and intensity override.
pub fn simplify_lossless_with_material(
    octree: &mut VoxelTree<Vector4<u8>>,
    save_data: &mut SaveData,
    opts: &Obj2Brz,
    max_merge: isize,
    material: Material,
    intensity: u8,
) {
    // Convert octree to flat grid for O(1) access
    let mut grid = VoxelGrid::from_octree(octree, &opts.logger);
    
    let colorset = convert_colorset_to_hsv(&save_data.colors);

    let scales: (isize, isize, isize) = if opts.bricktype == BrickType::Microbricks {
        (opts.brick_scale, opts.brick_scale, opts.brick_scale)
    } else {
        (5, 5, 2)
    };

    let max_scale = isize::max(isize::max(scales.0, scales.1), scales.2);
    let max_merge = max_merge / max_scale;
    let grid_len = grid.size as isize;

    let mut search_idx = 0usize;
    
    while let Some((idx, x, y, z, first_color)) = grid.find_next_voxel(search_idx) {
        search_idx = idx;
        
        let final_color = gamma_correct(first_color);
        let matched_color = match_hsv_to_colorset(&colorset, &rgb2hsv(final_color));
        let unmatched_color = BrickColor::Unique(Color::new(
            final_color[0],
            final_color[1],
            final_color[2],
        ));

        let mut xp = x + 1;
        let mut yp = y + 1;
        let mut zp = z + 1;

        // Expand z direction
        while zp - z < max_merge && zp < grid_len {
            if let Some(color) = grid.get(x, y, zp) {
                let c = gamma_correct(color);
                let m = match_hsv_to_colorset(&colorset, &rgb2hsv(c));
                if m != matched_color {
                    break;
                }
                zp += 1;
            } else {
                break;
            }
        }

        // Expand y direction
        while yp - y < max_merge && yp < grid_len {
            let mut pass = true;
            for sz in z..zp {
                if let Some(color) = grid.get(x, yp, sz) {
                    let c = gamma_correct(color);
                    let m = match_hsv_to_colorset(&colorset, &rgb2hsv(c));
                    if m != matched_color {
                        pass = false;
                        break;
                    }
                } else {
                    pass = false;
                    break;
                }
            }
            if !pass {
                break;
            }
            yp += 1;
        }

        // Expand x direction
        while xp - x < max_merge && xp < grid_len {
            let mut pass = true;
            'outer: for sy in y..yp {
                for sz in z..zp {
                    if let Some(color) = grid.get(xp, sy, sz) {
                        let c = gamma_correct(color);
                        let m = match_hsv_to_colorset(&colorset, &rgb2hsv(c));
                        if m != matched_color {
                            pass = false;
                            break 'outer;
                        }
                    } else {
                        pass = false;
                        break 'outer;
                    }
                }
            }
            if !pass {
                break;
            }
            xp += 1;
        }

        // Clear voxels in grid
        for sx in x..xp {
            for sy in y..yp {
                for sz in z..zp {
                    grid.clear(sx, sy, sz);
                }
            }
        }

        let color = if opts.match_brickadia_colorset {
            BrickColor::Index(matched_color as u32)
        } else {
            unmatched_color
        };

        let width = xp - x;
        let height = yp - y;
        let depth = zp - z;

        // Check if this brick has a top surface (no voxels directly above it)
        let is_top_surface = if opts.use_smooth_bricks {
            let mut has_voxel_above = false;
            'top_check: for sx in x..xp {
                for sy in y..yp {
                    if grid.get(sx, sy, zp).is_some() {
                        has_voxel_above = true;
                        break 'top_check;
                    }
                }
            }
            !has_voxel_above
        } else {
            false
        };

        save_data.bricks.push(create_brick_with_material_and_surface(
            opts,
            &save_data.colors,
            scales,
            (width, depth, height),
            (x, z, y),
            color,
            material,
            intensity,
            is_top_surface,
        ));
    }
}
