use crate::color::*;
use crate::octree::{Branches, TreeBody, VoxelTree};
use crate::{BrickType, Obj2Brz, SaveData};

use brdb::{Brick, BrickSize, BrickType as BrdbBrickType, Color, Direction, Position, Rotation};
use cgmath::Vector4;

/// Generate bricks directly from octree without VoxelGrid allocation.
/// This is used for very large models where VoxelGrid would cause memory overflow.
pub fn generate_bricks_direct(
    octree: &VoxelTree<Vector4<u8>>,
    save_data: &mut SaveData,
    opts: &Obj2Brz,
) {
    let scales: (isize, isize, isize) = if opts.bricktype == BrickType::Microbricks {
        (opts.brick_scale, opts.brick_scale, opts.brick_scale)
    } else {
        (5, 5, 2)
    };

    // Clone palette to avoid borrow conflict
    let palette = save_data.colors.clone();

    // Recursively traverse octree and generate one brick per voxel
    let half_size = 1isize << octree.size;
    traverse_and_generate(
        &octree.contents,
        half_size,
        -half_size,
        -half_size,
        -half_size,
        &palette,
        scales,
        opts,
        save_data,
    );
}

fn traverse_and_generate(
    branches: &Branches<Vector4<u8>>,
    mask: isize,
    base_x: isize,
    base_y: isize,
    base_z: isize,
    palette: &[Color],
    scales: (isize, isize, isize),
    opts: &Obj2Brz,
    save_data: &mut SaveData,
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
                    traverse_and_generate(b, m, x, y, z, palette, scales, opts, save_data);
                }
            }
            TreeBody::Leaf(color) => {
                if m == 0 {
                    // Generate a single brick for this voxel
                    // Note: Position is (x, z, y) to match simplify.rs coordinate swap
                    // Size is (width, depth, height) = (1, 1, 1) for single voxel
                    save_data.bricks.push(create_brick(
                        opts,
                        palette,
                        scales,
                        (1, 1, 1), // size = (width, depth, height)
                        (x, z, y), // position swap: Y and Z swapped for Brickadia coords
                        *color,
                    ));
                }
            }
            TreeBody::Empty => {}
        }
    }
}

fn create_brick(
    opts: &Obj2Brz,
    palette: &[Color],
    scale: (isize, isize, isize),
    size: (isize, isize, isize),
    pos: (isize, isize, isize),
    color: Vector4<u8>,
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
    } else if opts.bricktype == BrickType::Tiles || opts.use_smooth_bricks {
        "PB_DefaultSmoothTile"
    } else {
        "PB_DefaultBrick"
    };

    let brick_type = BrdbBrickType::from((asset_name, brick_size));

    // Convert color
    let final_color = gamma_correct(color);
    let brick_color = if opts.match_brickadia_colorset {
        let colorset = convert_colorset_to_hsv(palette);
        let matched_idx = match_hsv_to_colorset(&colorset, &rgb2hsv(final_color));
        if (matched_idx as usize) < palette.len() {
            palette[matched_idx as usize]
        } else {
            Color::new(final_color[0], final_color[1], final_color[2])
        }
    } else {
        Color::new(final_color[0], final_color[1], final_color[2])
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
