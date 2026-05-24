#!/bin/bash
# PEPAI BUILDER MODE: SYSTEM-WIDE REPAIR
set -e

echo "🛠️  [BUILDER]: Initiating FULL SYSTEM Repair..."

# 1. Define Paths
BRAIN_DIR="/media/bitboi/DATA1/Helheim_Brain"
BODY_DIR="/media/bitboi/DATA1/Helheim/helheim-cli"
SOCKET="/tmp/native_engine_brain.sock"

# 2. Stop Services
echo "🛑 [SYSTEM]: Stopping ALL services..."
sudo systemctl stop helheim_brain pepai pepai_brain || true

# 3. Rebuild Brain (The Engine)
echo "🧠 [COMPILER]: Rebuilding Helheim Brain (1MB Buffer + Native Socket)..."
cd "$BRAIN_DIR"
cargo build --release

# 4. Rebuild Body (The CLI)
echo "💪 [COMPILER]: Rebuilding Helheim Body (Socket Path Fix)..."
cd "$BODY_DIR"
cargo build --release

# 5. Install Body (Overwrite system binary)
echo "📦 [INSTALL]: Remove old symlink/binary..."
sudo rm -f /usr/local/bin/helheim
echo "📦 [INSTALL]: Installing new binary..."
sudo cp "$BODY_DIR/target/release/helheim-cli" /usr/local/bin/helheim
sudo chmod +x /usr/local/bin/helheim

# 6. Clean Socket
if [ -S "$SOCKET" ]; then
    echo "🧹 [CLEANUP]: Removing stale socket..."
    rm "$SOCKET"
fi

# 7. Restart Services
echo "🚀 [SYSTEM]: Restarting Helheim Brain..."
sudo systemctl start helheim_brain
sleep 3

echo "🔍 [CHECK]: Verifying Socket..."
if [ -S "$SOCKET" ]; then
    echo "✅ Socket active at $SOCKET"
    ls -l "$SOCKET"
    # Ensure wide permissions for the socket so WebApp can use it
    sudo chmod 777 "$SOCKET" 
else
    echo "❌ Socket NOT created. Checking logs..."
    sudo journalctl -u helheim_brain --no-pager -n 20
    exit 1
fi

echo "🚀 [SYSTEM]: Restarting PEPAI WEB (pepai_brain) & CORE (pepai)..."
sudo systemctl start pepai_brain pepai

echo "✅ [DONE]: FULL SYSTEM SYNC COMPLETE."
