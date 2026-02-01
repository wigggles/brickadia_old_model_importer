# obj2brs (Brickadia OBJ -> BRZ converter)

## What this repo is

This repository contains `obj2brs`, a Rust tool that converts a 3D model exported as a Wavefront `.obj` (optionally with `.mtl` + texture images) into a Brickadia save file (`.brz`). The generated save is a voxelized version of the model built out of Brickadia bricks/tiles/microbricks.

This is an adaptation for Brickadia “a5” of Suficio’s `textured-voxelizer`.

https://en.wikipedia.org/wiki/Wavefront_.obj_file

## What it outputs

- Primary output: a Brickadia save file in `.brz` format.
- Output path: `<output_directory>/<save_name>.brz`.
- Default output directory:
   - Windows: `%LOCALAPPDATA%\Brickadia\Saved\Builds` (falls back to `builds` if not found)
   - Linux: `~/.config/Epic/Brickadia/Saved/Builds` (falls back to `builds` if not found)

Internally the tool builds up a `brdb::World` and writes it using `brdb`.

## External Brickadia notes (helpful context)

These notes are based on official Brickadia blog posts and are included to provide context for how the generated files relate to the game.

1. **`.brs` vs `.brz` / prefabs**
   - Brickadia has been transitioning away from legacy `.brs` save files toward a newer system based on the “worlds” format.
   - Source: https://brickadia.com/blog/whats-next-for-brickadia/ (section “New Prefab System”, mentions “brs -> brz converter” and the newer format).

2. **Brick/building constraints that can affect conversions**
   - Brickadia Alpha 5 reduced the maximum brick size from 200 studs to 100 studs; oversized cuboids may be split.
   - This matters for converters because aggressive merging could attempt to create very large bricks.
   - Source: https://brickadia.com/blog/alpha-5/ (Detailed Changelog, Building; “Reduced the maximum brick size…”).

3. **Save management / backups**
   - Alpha 5 Patch 5 added save backups when overwriting or deleting a build/folder, and includes a UI button to open the backup folder.
   - Source: https://brickadia.com/blog/brickadia-alpha-5-patch-5/ (Detailed Changelog CL7420, UI, “Load and Save Bricks”).

## How conversion works (high level)

The pipeline is:

1. **Load OBJ**
   - Uses `tobj` to load geometry (triangulated) and materials.
   - If materials are present:
     - For each material, either load its diffuse texture image (via `image`) or fall back to the material diffuse color.
   - If no materials exist, a default solid white “texture” is used.

2. **Scale / normalize geometry**
   - Applies the user `Scale`.
   - Applies an extra Y-scale when using non-microbricks (to account for Brickadia plate height).
   - Raises the mesh upward so no vertices are negative on Z.

3. **Voxelize triangles into an octree** (`src/voxelize.rs`)
   - Computes the model AABB.
   - Expands an octree to contain the full bounds.
   - Recursively subdivides space; at the leaf level it samples color from the material texture using interpolated UVs.

4. **Simplify voxels into Brickadia bricks** (`src/simplify.rs`)
   - Converts filled voxels into a set of `brdb::Brick` instances.
   - Two modes:
     - **Lossless**: merges only regions of identical matched color.
     - **Lossy** (“Simplify”): merges regions using an averaged color (optionally matched to Brickadia palette).
   - Brick sizing differs by `Bricktype`:
     - `Microbricks`: uses `brick_scale` as the scale.
     - `Default` / `Tiles`: uses a fixed scale (currently `(5,5,2)` in the simplifier).

5. **Write `.brz`** (`src/brdb_support.rs`)
   - Builds a `brdb::World`.
   - Sets bundle metadata (name, author, description).
   - Embeds a preview image (converted to JPEG) for the save.
   - Writes the `.brz` file.

## UI / operation

This repo is currently an **egui/eframe desktop app** (not a CLI-first tool):

- Pick an input `.obj` file.
- Pick an output directory.
- Provide a save name.
- Choose options (simplify, scale, brick type, material, etc.).
- Click **Voxelize**.

## Notable options (from the UI)

- `Lossy Conversion` / `Simplify`: reduces brick count by merging.
- `Scale`: scales the model before voxelization.
- `Bricktype`: `Microbricks`, `Default`, or `Tiles`.
- `Material` and `material_intensity`: Brickadia material selection.
- `Match Brickadia colorset`: maps generated colors to a fixed palette instead of writing unique colors.
- `Split by material`: voxelize and export each material as a separate frozen grid (with configurable per-grid offset).

## Build / run

### Requirements

- Rust toolchain (edition 2021)
- The Rust dependencies are managed via `Cargo.toml`.

### Build (native)

```bash
cargo build --release
```

### Run (developer)

```bash
cargo run
```

## Cross-compiling / release builds

This repo includes scripts for cross-compilation:

- `setup-cross-compile.sh`: adds rust targets and installs MinGW-w64 on Linux.
- `build.sh`: builds release binaries for:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-pc-windows-gnu`

Outputs go to `dist/`.

See `BUILD.md` for details.

## Project layout (key files)

- `src/main.rs`
  - App state/options (`Obj2Brs`)
  - UI and conversion orchestration
  - `.obj` validation and resource checks
  - calls voxelization + simplification + `.brz` writing
- `src/voxelize.rs`
  - voxelization logic (triangle intersection + UV sampling)
- `src/simplify.rs`
  - converts voxel octree to `brdb::Brick` list (lossy/lossless)
- `src/brdb_support.rs`
  - writes `brdb::World` to `.brz`
- `BUILD.md`, `build.sh`, `setup-cross-compile.sh`
  - build and cross-compile instructions

## Common input pitfalls

- **Missing `.mtl`**: the OBJ may load but will be treated as having no materials.
- **Missing textures**: if `.mtl` references images that aren’t next to the OBJ (or paths are wrong), conversion may fail (or prompt to continue without textures depending on UI flow).
- **No UVs**: if the mesh has no UV coordinates, texture sampling can’t occur; material color / defaults are used.

## Status

- This summary is based on the current Rust sources in `src/` and the build scripts in the repo.
