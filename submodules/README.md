# Submodules

This directory contains third-party tools vendored as Git submodules. They are used as *supporting converters* in the asset pipeline that ultimately targets Brickadia.

## bsp-converter-obj_textured

- Purpose: convert certain game map formats (`.bsp`, `.d3dbsp`) into Wavefront `.obj`, and export textures referenced by the map.
- Upstream README: `submodules/bsp-converter-obj_textured/README.md`
- Upstream description (from the project): “CoD (1, 2) .bsp to .obj converter … exports textures … generates config.json for setting up game directories”.

https://github.com/kartjom/bsp-converter

### How it relates to this repo

If you have a source map in a supported BSP format, you can:

1. Convert BSP -> OBJ (and extract textures) using this submodule.
2. Feed the resulting OBJ (+ textures/materials) into `obj2brs` to produce a Brickadia `.brz` build.

## Notes

- Submodules are maintained separately from the main repo. If you update them, commit the submodule pointer change in this repo.
- Each submodule may have its own license and usage requirements; check the submodule’s README/LICENSE.
