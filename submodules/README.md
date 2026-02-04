# Submodules

This directory contains third-party tools vendored as Git submodules. They are used as *supporting converters* in the asset pipeline that ultimately targets Brickadia.

## source_gold-converter-obj (Python)

- **Language**: Python
- **Purpose**: Convert GoldSrc/Quake/Half-Life BSP maps to Wavefront `.obj` with textures.
- **Supported games**: Quake 1, Quake 2, **Half-Life 1** (GoldSrc)
- **Upstream**: https://github.com/measuredweighed/bsp2obj
- **Upstream README**: `submodules/source_gold-converter-obj/README.md`

### How it relates to this repo

This is the **primary reference** for Half-Life/GoldSrc BSP support. The Rust `plugins/bsp_converter` module is based on this Python implementation.

Usage (Python):
```bash
pip install bsp2obj
bsp2obj -g hl1 -o output_name -m path/to/map.bsp -c path/to/palette.lmp
```

## bsp-converter-obj_textured (C#/.NET)

- **Language**: C# (.NET)
- **Purpose**: Convert CoD/id Tech 3 BSP maps (`.bsp`, `.d3dbsp`) to Wavefront `.obj` with textures.
- **Supported games**: Call of Duty 1, Call of Duty 2, Medal of Honor: Allied Assault
- **Upstream**: https://github.com/kartjom/bsp-converter
- **Upstream README**: `submodules/bsp-converter-obj_textured/README.md`
- **Upstream description (from the project)**: “CoD (1, 2) .bsp to .obj converter … exports textures … generates config.json for setting up game directories”.

### How it relates to this repo

If you have a source map in a supported BSP format, you can:

1. Convert BSP -> OBJ (and extract textures) using this submodule.
2. Feed the resulting OBJ (+ textures/materials) into `obj2brz` to produce a Brickadia `.brz` build.

## Notes

- Submodules are maintained separately from the main repo. If you update them, commit the submodule pointer change in this repo.
- Each submodule may have its own license and usage requirements; check the submodule’s README/LICENSE.

## Alternative: Rust-native plugins

If you prefer a single-toolchain (Rust-only) workflow, see `plugins/` in the repo root.

The `plugins/bsp_converter/` crate is a Rust port of this submodule's functionality, removing the need for .NET/C# at runtime.
