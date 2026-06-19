#!/bin/bash
set -e

echo "=========================================="
echo "   HELHEIM CPU-ONLY TEST SUITE RUNNER     "
echo "=========================================="
echo "Zorgt ervoor dat de taal en core compiler volledig"
echo "draaien zonder afhankelijkheid van NVIDIA CUDA."
echo "=========================================="
echo ""

export HELHEIM_DISABLE_GPU=1
# Run tests across the workspace with default features disabled (disables 'cuda')
cargo test --workspace --no-default-features

echo ""
echo "✅ CPU-only integratie tests geslaagd!"
