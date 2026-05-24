#!/bin/bash
# PEPAI BUILDER MODE: AUTO-REPAIR SCRIPT (CORRECTED)
set -e

echo "🛠️  [BUILDER]: Initiating Brain Repair Protocol..."

# 1. Start the correct Web Interface Service
echo "🚀 [SYSTEM]: Starting PEPAI WEB (pepai_brain)..."
sudo systemctl restart pepai_brain

echo "✅ [DONE]: Web Interface (Port 3000) should be UP."
