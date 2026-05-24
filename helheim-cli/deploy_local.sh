#!/bin/bash
set -e

# Detect real user (who invoked sudo)
REAL_USER=${SUDO_USER:-$USER}

# Fallback: If still root, force 'bitboi' if home exists (Hard patch for this specific issue)
if [ "$REAL_USER" == "root" ] && [ -d "/home/bitboi" ]; then
    REAL_USER="bitboi"
fi

echo "🚀 DEPLOYING HELHEIM (LOCAL FIX)..."
echo "   Target Service User: $REAL_USER"

# Paths
BRAIN_DIR="/media/bitboi/DATA1/Helheim_Brain"
CLI_DIR="/media/bitboi/DATA1/Helheim/helheim-cli"

# 1. Stop Services
echo "[STOP] Stopping Helheim Services..."
sudo systemctl stop helheim-brain || true
sudo systemctl stop helheim-body || true

# 2. Install Binaries
echo "[INSTALL] Installing Binaries..."
# Ensure /usr/local/bin exists
sudo mkdir -p /usr/local/bin
sudo cp "$BRAIN_DIR/target/release/Helheim_Brain" /usr/local/bin/helheim-brain
sudo cp "$CLI_DIR/target/release/helheim-cli" /usr/local/bin/helheim

# 3. Create Service Files (Dynamic)
echo "[CONFIG] Generating Service Files for User: $REAL_USER..."

cat <<EOF | sudo tee /etc/systemd/system/helheim-body.service
[Unit]
Description=Helheim Body (The Engine)
After=network.target

[Service]
Type=simple
User=$REAL_USER
ExecStart=/usr/local/bin/helheim service --port 9001
Restart=always
RestartSec=5
Environment=RUST_LOG=info
Environment=HELHEIM_ROOT=$CLI_DIR

[Install]
WantedBy=multi-user.target
EOF

cat <<EOF | sudo tee /etc/systemd/system/helheim-brain.service
[Unit]
Description=Helheim Brain (The Intelligence)
After=network.target helheim-body.service

[Service]
Type=simple
User=$REAL_USER
WorkingDirectory=$BRAIN_DIR
ExecStartPre=/bin/rm -f /tmp/helheim_brain.sock
ExecStart=/usr/local/bin/helheim-brain
Restart=always
RestartSec=10
Environment="RAYON_NUM_THREADS=8"

[Install]
WantedBy=multi-user.target
EOF

# 4. Reload and Start
echo "[START] Reloading and Starting Services..."
sudo systemctl daemon-reload
# FORCE CLEANUP: Ensure any old root-owned socket is gone so 'bitboi' can create a new one
if [ -e "/tmp/helheim_brain.sock" ]; then
    echo "[CLEANUP] Removing stale root-owned socket..."
    sudo rm -f /tmp/helheim_brain.sock
fi
sudo systemctl enable helheim-brain
sudo systemctl enable helheim-body
sudo systemctl restart helheim-brain
sudo systemctl restart helheim-body

echo "✅ DEPLOYMENT COMPLETE."
echo "   Brain Status: sudo systemctl status helheim-brain"
echo "   Body Status:  sudo systemctl status helheim-body"
