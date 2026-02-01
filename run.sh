#!/bin/bash

# Launch script for obj2brs GUI
# Runs the application from source code

set -e  # Exit on error

echo "==================================="
echo "Launching obj2brs GUI..."
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
echo "obj2brs GUI closed"
echo "==================================="
