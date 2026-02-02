# obj2brs

a5 adaptation of [textured-voxelizer](https://github.com/CheezBarger/textured-voxelizer) by Suficio

![Voxelized plane](banner.png)

![Rampified import](banner2.png)

Generates textured voxel models from OBJ files.

## Quick start

This project builds a desktop GUI app that converts `.obj` files into Brickadia `.brz` saves.

Also supports `.bsp` files with plugins/bsp_converter.

**See [`DATA_FOLDERS.md`](DATA_FOLDERS.md) for details on where imports, exports, and logs are stored.**

### Run (Windows / PowerShell)

```powershell
cargo run
```

### Build release (Windows / PowerShell)

```powershell
cargo build --release
```

Then run:

```powershell
./target/release/obj2brs.exe
```

## Build scripts

The repo includes bash scripts intended for Linux/WSL environments:

- `setup-cross-compile.sh`: installs Rust targets and cross-compile dependencies (see `BUILD.md`).
- `build.sh`: produces release binaries into `dist/` (see `BUILD.md`).

On Windows, you typically run these via WSL or Git Bash.

## Submodules

This repo vendors some supporting conversion tools as Git submodules under `submodules/`.

### Cloning with submodules

```bash
git clone --recurse-submodules <repo-url>
```

If you already cloned without them:

```bash
git submodule update --init --recursive
```

### Where to find / how to use

- Submodule code lives in `submodules/`
- Each tool has its own README and usage. Start here: `submodules/README.md`

## Plugins (Rust-native converters)

The `plugins/` directory contains Rust-based conversion tools that can replace external dependencies:

- `plugins/bsp_converter/` — Rust port of BSP-to-OBJ conversion (replaces the C# submodule)

See `plugins/README.md` for build instructions and `plugins/bsp_converter/DESIGN.md` for architecture notes.

## More details

- `SUMMARY.md`: overview of the conversion pipeline, options, and output behavior.
- `BUILD.md`: multi-platform build notes.
- `DATA_FOLDERS.md`: where imports, exports, and logs are stored for each run mode.
- `SETUP_WINDOWS_RUST.md`: Windows 11 + Rust + PowerShell setup guide.
- `plugins/README.md`: Rust-native converter plugins.



