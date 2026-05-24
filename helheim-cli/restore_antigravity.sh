#!/bin/bash
# PEPAI BUILDER MODE: RESTORE ANTIGRAVITY (ULTRAVIOLET)
set -e

echo "🛠️  [BUILDER]: Restoring Ultraviolet System State..."

# 1. Paths
BRAIN_DIR="/media/bitboi/DATA1/Helheim_Brain"
# This is the path to the USER'S optimized local binary
ULTRAVIOLET_BIN="/home/bitboi/.gemini/antigravity/playground/ultraviolet-hubble/helheim-cli/target/release/helheim-cli"
SOCKET="/tmp/helheim_brain.sock"

# 2. Stop Services
echo "🛑 [SYSTEM]: Stopping services..."
sudo systemctl stop helheim_brain pepai pepai_brain || true

# 3. Restore Symlink (Crucial Step)
if [ -f "$ULTRAVIOLET_BIN" ]; then
    echo "🔗 [LINK]: Restoring symlink to Ultraviolet Hubble..."
    sudo rm -f /usr/local/bin/helheim
    sudo ln -s "$ULTRAVIOLET_BIN" /usr/local/bin/helheim
    sudo chmod +x /usr/local/bin/helheim
    echo "✅ [LINK]: Symlink restored."
    ls -l /usr/local/bin/helheim
else
    echo "⚠️  [WARNING]: Could not find Ultraviolet binary at $ULTRAVIOLET_BIN"
    echo "   Continuing with Brain rebuild, but 'helheim' command might be broken."
fi

# 4. Rebuild Brain (With Reverted Socket)
echo "🧠 [COMPILER]: Rebuilding Helheim Brain (1MB Buffer + Default Socket)..."
if [ -d "$BRAIN_DIR" ]; then
    cd "$BRAIN_DIR"
    cargo build --release
else
    echo "❌ ERROR: Cannot find Brain directory $BRAIN_DIR"
    exit 1
fi

# 5. Clean Socket
if [ -S "$SOCKET" ]; then
    echo "🧹 [CLEANUP]: Removing stale socket $SOCKET..."
    rm "$SOCKET"
fi

# 6. Restart Services
echo "🚀 [SYSTEM]: Restarting Helheim Brain..."
sudo systemctl start helheim_brain
sleep 3

echo "🔍 [CHECK]: Verifying Socket..."
if [ -S "$SOCKET" ]; then
    echo "✅ Socket active at $SOCKET"
    ls -l "$SOCKET"
    sudo chmod 777 "$SOCKET"
else
    echo "❌ Socket NOT created. Checking logs..."
    sudo journalctl -u helheim_brain --no-pager -n 20
    exit 1
fi

echo "🚀 [SYSTEM]: Restarting PEPAI System..."
# Try starting both, ignoring errors if one doesn't exist (like 'pepai')
sudo systemctl start pepai_brain || true
sudo systemctl start pepai || true

echo "✅ [DONE]: Antigravity Restored."
