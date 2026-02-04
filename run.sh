#!/bin/bash

# Launch script for obj2brz GUI
# Runs the application from source code

set -e  # Exit on error

echo "==================================="
echo "Launching obj2brz GUI..."
echo "==================================="

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo not found. Please install Rust from https://rustup.rs/"
    exit 1
fi

# Run the GUI application
cargo run --release

echo ""
echo "==================================="
echo "obj2brz GUI closed"
echo "==================================="
