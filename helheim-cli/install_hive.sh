#!/bin/bash
set -e

# Function to check command existence
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

echo "🚀 HELHEIM HIVE DEPLOYMENT INITIATED..."

# 0. Check for Rust/Cargo
if ! command_exists cargo; then
    echo "⚠️  Cargo (Rust Compiler) not found in PATH."
    if [ -f "$HOME/.cargo/env" ]; then
        echo "🔄 Sourcing $HOME/.cargo/env..."
        source "$HOME/.cargo/env"
    fi
fi

if ! command_exists cargo; then
    echo "❌ Cargo is still not found. Rust is likely not installed on this server."
    echo "👉 Installing Rust (Rustup)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Double check
if ! command_exists cargo; then
    echo "❌ Failed to install/find Rust. Please install it manually."
    exit 1
fi

echo "✅ Rust detected. Proceeding with build..."

# 1. Build Helheim Body (CLI/Service)
echo "[BUILD] Compiling Helheim Body (Release Mode)..."
# Ensure we are compiling as the USER, not root (unless user is root)
cd $HOME/Helheim/helheim-cli
cargo build --release --bin helheim-cli

# 2. Build Helheim Brain (CPU Mode)
echo "[BUILD] Compiling Helheim Brain (CPU Mode - No GPU)..."
cd $HOME/Helheim_Brain
# Ensure we build without CUDA features (default is now CPU due to Cargo.toml change)
cargo build --release

# 3. Install Binaries (REQUIRES SUDO)
echo "[INSTALL] Stopping services to release file locks..."
sudo systemctl stop helheim-brain || true
sudo systemctl stop helheim-body || true

echo "[INSTALL] Moving binaries to /usr/local/bin (Password may be required)..."
sudo cp $HOME/Helheim/helheim-cli/target/release/helheim-cli /usr/local/bin/helheim
sudo cp $HOME/Helheim_Brain/target/release/Helheim_Brain /usr/local/bin/helheim-brain

# 4. Install Service Files (REQUIRES SUDO)
echo "[SYSTEMD] Configuring Services..."
sudo cp $HOME/Helheim/helheim-cli/deploy/helheim-body.service /etc/systemd/system/
sudo cp $HOME/Helheim/helheim-cli/deploy/helheim-brain.service /etc/systemd/system/

# 5. Enable and Start (REQUIRES SUDO)
echo "[IGNITION] Reloading Daemon and Starting Services..."
sudo systemctl daemon-reload
sudo systemctl enable helheim-body
sudo systemctl enable helheim-brain
sudo systemctl restart helheim-body
sudo systemctl restart helheim-brain

echo "✅ HIVE DEPLOYMENT COMPLETE."
echo "   Monitor Body:  sudo systemctl status helheim-body"
echo "   Monitor Brain: sudo systemctl status helheim-brain"
