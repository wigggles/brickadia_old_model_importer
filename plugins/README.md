# Plugins

This directory contains Rust-based conversion plugins that extend the `obj2brs` pipeline.

## Purpose

The goal is to provide native Rust implementations of format converters (e.g., BSP → OBJ) so that:

1. The entire pipeline from source format → OBJ → Brickadia `.brz` can be built with a single toolchain (Rust/Cargo).
2. No external C#/.NET dependencies are required at runtime.
3. Code can be shared/integrated more easily with the main `obj2brs` crate if desired.

## Current plugins

- `bsp_converter/` — Rust port of BSP-to-OBJ conversion (inspired by the C# `bsp-converter-obj_textured` submodule).

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
