#!/bin/sh
# branch-tone installer
# Usage: curl -sSL https://raw.githubusercontent.com/rmzi/branch-tone/main/install.sh | sh
set -e

echo "Installing branch-tone..."
echo ""

# Check for cargo
if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: cargo not found."
    echo "Install Rust: https://rustup.rs"
    exit 1
fi

# Install from git
cargo install --git https://github.com/rmzi/branch-tone

# Verify
if ! command -v branch-tone >/dev/null 2>&1; then
    echo ""
    echo "Warning: branch-tone installed but not found in PATH."
    echo "Ensure ~/.cargo/bin is in your PATH."
    exit 1
fi

echo ""
echo "branch-tone $(branch-tone --version) installed successfully!"
echo ""
echo "Run 'branch-tone init' to register Claude Code hooks."
