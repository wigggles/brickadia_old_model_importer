# BSP Converter — Design Document

This document outlines the design for a Rust-based BSP-to-OBJ converter, inspired by the C# `bsp-converter-obj_textured` submodule.

## Goals

1. Parse BSP files from supported games (initially CoD1, CoD2, MoHAA).
2. Extract geometry (vertices, normals, UVs, faces) and material references.
3. Export to Wavefront OBJ format (with accompanying MTL file).
4. Optionally extract textures from game archives (PK3/ZIP).
5. Pure Rust — no C#/.NET runtime required.

## Reference: C# submodule structure

The existing C# implementation has these key components:

### BinaryFileReader (BinLib.cs)
- Generic binary struct reading via marshalling
- Stream position management
- Byte-to-struct conversion

### Decompiler
- `IBSP` interface: common trait for all BSP format handlers
- `SharedLumps.cs`: shared structs (Header, CoDLump, CoDMaterial, Vertex types, TriangleSoups, MeshVerts)
- `BSP Formats/`: per-game implementations (CoD1BSP, CoD2BSP, MoHAABSP)
- `Program.cs`: CLI entry point

### IWI_DDS_Converter
- Converts IWI texture format to DDS
- Writes DDS header + mipmap data

## Rust module structure

```
plugins/bsp_converter/
├── Cargo.toml
├── DESIGN.md          # This file
├── src/
│   ├── lib.rs         # Crate root, re-exports
│   ├── binary.rs      # Binary reading utilities (like BinLib)
│   ├── bsp/
│   │   ├── mod.rs     # BSP module root
│   │   ├── header.rs  # BSP header parsing
│   │   ├── lump.rs    # Lump definitions and reading
│   │   ├── cod1.rs    # CoD1 BSP format
│   │   ├── cod2.rs    # CoD2 BSP format (placeholder)
│   │   └── mohaa.rs   # MoHAA BSP format (placeholder)
│   ├── obj/
│   │   ├── mod.rs     # OBJ export module root
│   │   ├── writer.rs  # OBJ file writer
│   │   └── mtl.rs     # MTL (material) file writer
│   └── texture/
│       ├── mod.rs     # Texture extraction module root
│       ├── pk3.rs     # PK3/ZIP archive reading
│       └── iwi.rs     # IWI to DDS conversion
```

## Key data structures (Rust equivalents)

### BSP Header
```rust
#[repr(C, packed)]
pub struct BspHeader {
    pub signature: [u8; 4],  // "IBSP"
    pub version: i32,        // 59 = CoD1, 4 = CoD2, etc.
}
```

### Lump entry
```rust
#[repr(C, packed)]
pub struct LumpEntry {
    pub length: i32,
    pub offset: i32,
}
```

### Vertex (CoD1)
```rust
#[repr(C, packed)]
pub struct Cod1Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub unknown1: [f32; 2],
    pub normal: [f32; 3],
    pub unknown2: f32,
}
```

### Triangle soup (face group)
```rust
#[repr(C, packed)]
pub struct TriangleSoup {
    pub material_id: u16,
    pub draw_order: u16,
    pub vertex_offset: i32,
    pub vertex_length: u16,
    pub triangle_length: u16,
    pub triangle_offset: i32,
}
```

## Conversion pipeline

1. **Open BSP file** → read header → determine version
2. **Read lump table** → locate data sections (materials, vertices, faces, mesh indices)
3. **Parse lumps** → deserialize into Rust structs
4. **Build OBJ data** → iterate faces, emit vertices/UVs/normals, emit face indices
5. **Write OBJ file** → `v`, `vt`, `vn`, `f` lines
6. **Write MTL file** → `newmtl`, `map_Kd` lines
7. **(Optional) Extract textures** → read PK3 archives, convert IWI → DDS if needed

## Dependencies (suggested)

- `bytemuck` or `zerocopy` — safe transmute for packed structs
- `zip` — reading PK3 archives
- `image` — texture format handling (optional)

## Status

- [x] Binary reading utilities
- [x] BSP header + lump parsing
- [x] CoD1 format implementation (scaffolded)
- [x] OBJ/MTL export
- [ ] Texture extraction (PK3)
- [ ] IWI → DDS conversion
- [ ] CoD2 format
- [ ] MoHAA format
- [ ] **GoldSrc/Quake/Half-Life format (PRIORITY)**

## Notes

- The C# code uses `Marshal.PtrToStructure` for binary reading; in Rust we use `#[repr(C, packed)]` structs with `bytemuck::from_bytes`.
- OBJ indices are 1-based; remember to add 1 when writing face lines.
- Texture paths in BSP may use `/` or `\`; normalize to `/`.

---

# GoldSrc / Quake / Half-Life BSP Format

This section documents the GoldSrc engine BSP format, which is the **priority target** for this converter.

## Reference

Based on the Python `bsp2obj` submodule at `submodules/source_gold-converter-obj`.

## Supported games

- Quake 1 (Q1) — version 29
- Half-Life 1 (HL1) — version 30 (GoldSrc)
- Quake 2 (Q2) — version 38 (different format)

## Header differences

GoldSrc/Quake BSP files do **NOT** have an "IBSP" signature like CoD/id Tech 3.

```
Q1/HL1 header:
- 4 bytes: version (int32) — 29 for Q1, 30 for HL1

Q2 header:
- 4 bytes: "IBSP" signature
- 4 bytes: version (int32) — 38
```

## Lump table

| Game | Lump count |
|------|------------|
| Q1/HL1 | 15 lumps |
| Q2 | 19 lumps |

### Q1/HL1 lump indices

| Index | Content |
|-------|---------|
| 2 | Textures (embedded mip textures) |
| 3 | Vertices |
| 6 | TexInfo |
| 7 | Faces |
| 12 | Edges |
| 13 | LEdges (surfedges) |

## Data structures

### Vertex (12 bytes)
```rust
#[repr(C, packed)]
pub struct GoldSrcVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
```

### Edge (4 bytes)
```rust
#[repr(C, packed)]
pub struct GoldSrcEdge {
    pub v1: u16,  // First vertex index
    pub v2: u16,  // Second vertex index
}
```

### LEdge / Surfedge (4 bytes)
```rust
// Signed int32 — if negative, edge is reversed
pub type SurfEdge = i32;
```

### Face (20 bytes)
```rust
#[repr(C, packed)]
pub struct GoldSrcFace {
    pub plane_id: u16,
    pub side: u16,
    pub first_edge: i32,      // Index into surfedges
    pub num_edges: i16,       // Number of edges
    pub texinfo_id: i16,      // Index into texinfo
    pub lightmap_styles: [u8; 4],
    pub lightmap_offset: i32,
}
```

### TexInfo (40 bytes for Q1/HL1)
```rust
#[repr(C, packed)]
pub struct GoldSrcTexInfo {
    pub u_axis: [f32; 3],
    pub u_offset: f32,
    pub v_axis: [f32; 3],
    pub v_offset: f32,
    pub texture_id: u32,
    pub flags: u32,
}
```

### Mip Texture header (40 bytes)
```rust
#[repr(C, packed)]
pub struct MipTexHeader {
    pub name: [u8; 16],
    pub width: u32,
    pub height: u32,
    pub offset1: u32,  // Mip level 0
    pub offset2: u32,  // Mip level 1
    pub offset4: u32,  // Mip level 2
    pub offset8: u32,  // Mip level 3
}
```

## Geometry reconstruction

GoldSrc uses **edge-based** geometry, not triangle soups:

1. Each face has `first_edge` and `num_edges`
2. Look up `surfedges[first_edge..first_edge+num_edges]`
3. Each surfedge is a signed index into edges:
   - Positive: use edge.v1 → edge.v2
   - Negative: use edge.v2 → edge.v1 (reversed)
4. This gives a polygon (fan of vertices)
5. Triangulate: for N vertices, emit N-2 triangles as fan from vertex 0

## UV calculation

UVs are **not stored per-vertex**. They are computed from texinfo:

```rust
let u = (vertex.dot(texinfo.u_axis) + texinfo.u_offset) / texture.width;
let v = (vertex.dot(texinfo.v_axis) + texinfo.v_offset) / texture.height;
```

## Coordinate swizzle

GoldSrc uses a different coordinate system. Apply swizzle:

```rust
fn swizzle(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    (x, z, -y)
}
```

## Embedded textures (Q1/HL1)

Textures are stored inside the BSP file:

1. Texture lump starts with `num_textures: u32`
2. Followed by `num_textures` offsets (u32 each)
3. Each offset points to a `MipTexHeader` + pixel data
4. Pixel data is palette-indexed (1 byte per pixel)
5. HL1: palette is embedded after mip level 3 data (256 * 3 bytes RGB)
6. Q1: palette is in external `palette.lmp` file

## Implementation plan

1. Add `src/bsp/goldsrc.rs` module
2. Add GoldSrc-specific structs (vertex, edge, face, texinfo, miptex)
3. Add edge-based geometry reconstruction
4. Add UV calculation from texinfo
5. Add coordinate swizzling
6. Add embedded texture parsing
7. Add palette loading (LMP format)
8. Update header detection to handle missing "IBSP" signature
