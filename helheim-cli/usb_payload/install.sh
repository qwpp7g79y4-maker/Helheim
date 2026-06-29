#!/bin/bash
# Helheim Installer (The Infector)
# Version: 1.0.0 (Seed)

set -e

echo "---------------------------------------------------"
echo "   HELHEIM: The Native Ascension (Installer)       "
echo "---------------------------------------------------"

BIN_DIR="$HOME/.local/bin"
SERVICE_DIR="$HOME/.config/systemd/user"
BIN_NAME="helheim"
SERVICE_NAME="helheim.service"

# 1. Prepare Environment
echo "[*] Preparing environment..."
mkdir -p "$BIN_DIR"
mkdir -p "$SERVICE_DIR"

# 2. Deploy Binary
echo "[*] Deploying Helheim Core to $BIN_DIR..."
if [ -f "./$BIN_NAME" ]; then
    cp "./$BIN_NAME" "$BIN_DIR/$BIN_NAME"
    chmod +x "$BIN_DIR/$BIN_NAME"
    echo "    -> Binary installed."
else
    echo "[!] ERROR: Binary './$BIN_NAME' not found in current directory!"
    exit 1
fi

# 3. Deploy Service
echo "[*] Deploying Persistence Daemon..."
if [ -f "./$SERVICE_NAME" ]; then
    cp "./$SERVICE_NAME" "$SERVICE_DIR/$SERVICE_NAME"
    echo "    -> Service unit moved to $SERVICE_DIR."
else
    echo "[!] WARNING: Service file missing. Persistence disabled."
fi

# 4. Activate Persistence
echo "[*] Activating Hive Node..."
systemctl --user daemon-reload
systemctl --user enable "$SERVICE_NAME"
systemctl --user restart "$SERVICE_NAME"

echo "---------------------------------------------------"
echo "   INFECTION COMPLETE. HELHEIM IS ACTIVE.          "
echo "---------------------------------------------------"
echo "Stats:"
systemctl --user status "$SERVICE_NAME" --no-pager | head -n 5
