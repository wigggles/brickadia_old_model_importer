use crate::{Obj2Brz, SaveData};
use crate::error::{ConversionError, ConversionResult};
use std::path::PathBuf;
use brdb::{Entity, World};

pub fn write_brz(
    path: PathBuf,
    data: &SaveData,
    opts: &Obj2Brz,
    _use_procedural: bool,
    preview_image: Option<Vec<u8>>,
) -> ConversionResult<()> {
    let mut world = World::new();

    // Set Metadata
    if let Some(img) = preview_image {
        world.meta.screenshot = Some(img);
    }

    // Set Bundle Info from SaveData
    if let Some(stem) = path.file_stem() {
        world.meta.bundle.name = stem.to_string_lossy().to_string();
    }
    world.meta.bundle.authors = vec![data.author_name.clone()];
    world.meta.bundle.description = "Converted with obj2brz".to_string();

    // Copy bricks directly - they're already in brdb format
    world.bricks = data.bricks.clone();

    // Update material intensity for all bricks
    for brick in &mut world.bricks {
        brick.material_intensity = opts.material_intensity as u8;
    }

    match world.write_brz(&path) {
        Ok(_) => {
            opts.logger.log(format!("Successfully wrote BRZ to {}", path.display()));
            Ok(())
        }
        Err(e) => Err(ConversionError::SaveWriteError(format!("Failed to write BRZ file: {:?}", e))),
    }
}

pub fn write_brz_grids(
    path: PathBuf,
    grids: Vec<(Entity, Vec<brdb::Brick>)>,
    opts: &Obj2Brz,
    preview_image: Option<Vec<u8>>,
) -> ConversionResult<()> {
    let mut world = World::new();

    // Set Metadata
    if let Some(img) = preview_image {
        world.meta.screenshot = Some(img);
    }

    // Set Bundle Info
    if let Some(stem) = path.file_stem() {
        world.meta.bundle.name = stem.to_string_lossy().to_string();
    }
    world.meta.bundle.authors = vec![opts.save_owner_name.clone()];
    world.meta.bundle.description = "Converted with obj2brz (split by material)".to_string();

    // Add all bricks directly to world.bricks instead of using frozen grids
    // Frozen grids have rendering issues in Brickadia - bricks don't appear
    // The Entity location offset is applied directly to brick positions
    let mut total_bricks = 0usize;
    for (entity, bricks) in grids {
        let offset_x = entity.location.x as i32;
        let offset_y = entity.location.y as i32;
        let offset_z = entity.location.z as i32;
        
        for mut brick in bricks {
            // Apply entity location offset to brick position
            brick.position.x += offset_x;
            brick.position.y += offset_y;
            brick.position.z += offset_z;
            world.bricks.push(brick);
            total_bricks += 1;
        }
    }

    // Update material intensity for all bricks
    for brick in &mut world.bricks {
        brick.material_intensity = opts.material_intensity as u8;
    }

    opts.logger.log(format!("Total bricks across all grids: {}", total_bricks));

    match world.write_brz(&path) {
        Ok(_) => {
            opts.logger.log(format!("Successfully wrote BRZ to {}", path.display()));
            Ok(())
        }
        Err(e) => Err(ConversionError::SaveWriteError(format!("Failed to write BRZ file: {:?}", e))),
    }
}
