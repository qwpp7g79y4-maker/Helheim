#!/bin/bash
# PEPAI BUILDER MODE: AUTO-REPAIR SCRIPT
set -e

echo "🛠️  [BUILDER]: Initiating Brain Repair Protocol..."

# 1. Define Paths
BRAIN_DIR="/media/bitboi/DATA1/Helheim_Brain"
SOCKET="/tmp/native_engine_brain.sock"

# 2. Stop Services
echo "🛑 [SYSTEM]: Stopping services..."
sudo systemctl stop helheim_brain pepai pepai_brain || true

# 3. Rebuild Brain (Force Release)
echo "🧠 [COMPILER]: Rebuilding Helheim Brain (with 1MB Buffer patch)..."
if [ -d "$BRAIN_DIR" ]; then
    cd "$BRAIN_DIR"
    cargo build --release
else
    echo "❌ ERROR: Cannot find directory $BRAIN_DIR"
    exit 1
fi

# 4. Clean Socket
if [ -S "$SOCKET" ]; then
    echo "🧹 [CLEANUP]: Removing stale socket $SOCKET..."
    rm "$SOCKET"
fi

# 5. Restart Services
echo "🚀 [SYSTEM]: Restarting Helheim Brain..."
sudo systemctl start helheim_brain
sleep 2

echo "🔍 [CHECK]: Verifying Socket..."
if [ -S "$SOCKET" ]; then
    echo "✅ Socket active at $SOCKET"
    ls -l "$SOCKET"
else
    echo "❌ Socket NOT created. Checking logs..."
    sudo journalctl -u helheim_brain --no-pager -n 20
    exit 1
fi

echo "🚀 [SYSTEM]: Restarting PEPAI Web..."
sudo systemctl start pepai
# sudo systemctl start pepai_brain # Uncomment if this is the active one

echo "✅ [DONE]: System repaired. Try chatting now."
