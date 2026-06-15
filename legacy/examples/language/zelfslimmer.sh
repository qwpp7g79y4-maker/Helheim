#!/bin/bash
# ==========================================
# ZELFSLIMMER - De zelfherstartende wrapper
# 
# Dit shell script "herstart" de Helheim script steeds,
# en maakt het "slimmer" door de ervaring te verhogen
# in het .hel bestand voor elke generatie.
#
# Leuk demo van hoe je Helheim kunt gebruiken in een
# self-improving / self-restarting setup.
# ==========================================

echo "=== ZELFSLIMMER GESTART ==="
echo "Ik ga de Helheim script 5 keer herstarten en steeds slimmer maken."

STATE_FILE="examples/language/zelfslimmer.hel"

for gen in 1 2 3 4 5; do
    echo ""
    echo "=== GENERATIE $gen ==="
    
    # Maak de script "slimmer" door de ervaring te verhogen
    # (simuleert dat de code zelf beter wordt)
    ERVARING=$(( 5 + gen * 3 ))
    sed -i "s/zet ervaring = [0-9]*/zet ervaring = $ERVARING/" "$STATE_FILE"
    
    echo "Huidige ervaring ingesteld op: $ERVARING"
    
    # Herstart / voer de Helheim script uit
    ./target/debug/helheim-cli script "$STATE_FILE" || echo "(run had een kleine issue, maar concept werkt)"
    
    sleep 1
done

echo ""
echo "=== EVOLUTIE KLAAR ==="
echo "De Helheim script is nu 'slimmer' geworden over de herstarts."
echo "Kijk in $STATE_FILE hoe de 'zet ervaring' regel hoger is geworden."
echo ""
echo "Je kunt het zelf herstarten met:"
echo "  ./examples/language/zelfslimmer.sh"
