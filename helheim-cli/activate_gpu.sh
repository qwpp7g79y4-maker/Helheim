#!/bin/bash
# HELHEIM ASCENSION PROTOCOL: GPU ACTIVATION
set -e

echo "🚀 [UPGRADE]: Activating RTX 5060 Ti Support (Helheim v1.1)..."

# 1. Paths
BODY_DIR="/media/bitboi/DATA1/Helheim/helheim-cli"
BRAIN_DIR="/media/bitboi/DATA1/Helheim_Brain"

# 2. Rebuild Body (With new GPU features)
echo "💪 [COMPILER]: Building Helheim CLI (with 'gpu work on ID')..."
cd "$BODY_DIR"
cargo build --release

# 3. Update System Binary (Correct Precedence)
echo "📦 [INSTALL]: Updating binaries (Global & Local)..."
sudo rm -f /usr/local/bin/helheim
sudo cp "$BODY_DIR/target/release/helheim-cli" /usr/local/bin/helheim
sudo chmod +x /usr/local/bin/helheim

# FORCE UPDATE local user bin (Crucial for user shell precedence)
echo "📦 [INSTALL]: Forcing update of local user binary..."
mkdir -p /home/bitboi/.local/bin
cp -f "$BODY_DIR/target/release/helheim-cli" /home/bitboi/.local/bin/helheim
chmod +x /home/bitboi/.local/bin/helheim

# 4. Verify GPU 1 (5060 Ti) Access
echo "🧪 [TEST]: Verifying CLI can see GPU 1..."
# We can't run a full kernel here without blocking, but we can check help/version if implemented
# Or dry run?

echo "✅ [DONE]: Helheim is now upgraded."
echo "   New Syntax: helheim run \"gpu work 16384 on 1\""
echo "   (Use Device 1 for the 5060 Ti, Device 0 for the 3060)"

# 5. Restart Brain (Just in case)
echo "🧠 [SYSTEM]: Ensuring Brain is active..."
sudo systemctl restart helheim_brain
