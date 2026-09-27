#!/bin/bash

# Build script for Project AEGIS - Layer 4 Risk Management System

echo "Building Project AEGIS - Layer 4 Risk Management System..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null
then
    echo "Cargo is not installed. Please install Rust first: https://www.rust-lang.org/"
    exit 1
fi

# Build in release mode for maximum performance
echo "Compiling in release mode..."
cargo build --release

if [ $? -eq 0 ]; then
    echo "Build successful!"
    echo "Executable located at: target/release/risk-management"
    echo ""
    echo "Documentation:"
    echo "  English: README.md"
    echo "  Arabic:  README.ar.md"
    echo ""
    echo "To run the system:"
    echo "  cd target/release"
    echo "  ./risk-management"
else
    echo "Build failed!"
    exit 1
fi