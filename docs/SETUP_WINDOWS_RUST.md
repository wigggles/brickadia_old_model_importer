# Windows Setup (Rust) for obj2brz

This guide walks through setting up and running `obj2brz` on **Windows 11** using **PowerShell**.

## Prerequisites

- Windows 10/11
- Internet access (Cargo downloads Rust crates from crates.io)

## 1) Install Rust

Install Rust via `rustup`:

- https://www.rust-lang.org/tools/install

During installation, choose the default option.

After installation, **close and re-open PowerShell** so `cargo`/`rustc` are on your PATH.

## 2) Verify Rust is installed

In PowerShell:

```powershell
rustc --version
cargo --version
```

If either command is not found, restart PowerShell and ensure Rust was installed for your user.

## 3) Clone the repo (with submodules)

If you are cloning fresh, prefer cloning with submodules:

```powershell
git clone --recurse-submodules <repo-url>
```

If you already cloned without submodules:

```powershell
git submodule update --init --recursive
```

Submodules (third-party tools) live in `submodules/`. See `submodules/README.md`.

## 4) Build and run the GUI

From the repository root:

```powershell
cargo run
```

This launches the desktop GUI. From there you can pick an input `.obj`, output directory, and generate a Brickadia `.brz` file.

### Build a release executable

```powershell
cargo build --release
```

Then run:

```powershell
./target/release/obj2brz.exe
```

## 5) Output location (default)

By default, `obj2brz` selects Brickadia’s Builds folder on Windows:

- `%LOCALAPPDATA%\Brickadia\Saved\Builds`

You can override it in the GUI.

## 6) About the `.sh` scripts on Windows

This repo includes:

- `setup-cross-compile.sh`
- `build.sh`

These are **bash scripts** intended for Linux-like environments.

On Windows you generally run them via one of:

- WSL (Windows Subsystem for Linux)
- Git Bash

For details, see `BUILD.md`.

## Troubleshooting

## A) PowerShell says a command is not recognized

Example:

- `car go run`

PowerShell treats the first word (`car`) as the command name. If it isn’t a real program, you’ll get “not recognized”.

Use:

```powershell
cargo run
```

## B) `cargo run` fails while resolving dependencies (example: `wgpu`)

Example error pattern:

- `failed to select a version for wgpu`
- `egui-wgpu ... depends on wgpu, with features: web-sys but wgpu does not have these features`

What it means:

- This is not a “Rust compiler” error in your code.
- Cargo is failing to find a **compatible set of crate versions/features**.

What to try:

1. Update Rust toolchain (recommended first step):

```powershell
rustup update stable
rustup default stable
```

2. Retry:

```powershell
cargo run
```

3. Once it resolves successfully, generate and commit a lockfile:

```powershell
cargo generate-lockfile
```

A `Cargo.lock` helps keep dependencies stable across machines, but it can only be created after Cargo can resolve versions at least once.

If it still fails after updating Rust, the project may need a small dependency version adjustment (e.g. pinning `eframe`/`wgpu` to a compatible set).

## C) Submodule tools are missing

If `submodules/` is empty or missing expected tools:

```powershell
git submodule update --init --recursive
```

## Next docs to read

- `README.md` (quick start)
- `SUMMARY.md` (how conversion works)
- `BUILD.md` (cross-compilation scripts)
- `submodules/README.md` (what each submodule does)
