# Plugins

This directory contains Rust-based conversion plugins that extend the `obj2brz` pipeline.

## Purpose

The goal is to provide native Rust implementations of format converters (e.g., BSP → OBJ) so that:

1. The entire pipeline from source format → OBJ → Brickadia `.brz` can be built with a single toolchain (Rust/Cargo).
2. No external C#/.NET dependencies are required at runtime.
3. Code can be shared/integrated more easily with the main `obj2brz` crate if desired.

## Current plugins

- `bsp_converter/` — Rust port of BSP-to-OBJ conversion (inspired by the C# `bsp-converter-obj_textured` submodule).

When running the bsp_converter, it will look for the game data in the following locations:
* Halflife 1: `data\game_textures\goldsrc_textures`


#### Many games are supported, here are some details related to those known tested games:

HalfLife 1:

what are WADs? https://github.com/yuraj11/HL-Texture-Tools
how Textures? https://tcrf.net/Half-Life_(Windows)/Textures

File Sources:
* VTF/VMT https://www.realm667.com/repository/texture-stock/other-sources-styles/884-half-life-texture-pack
* PNGs https://tf2maps.net/downloads/half-life-textures-goldsrc.17436/


## Building

From the repo root:

```powershell
cargo build -p bsp_converter
```

Or build all workspace members:
```powershell
cargo build --workspace
```

## Design notes

See `plugins/bsp_converter/DESIGN.md` for architecture and implementation notes.
