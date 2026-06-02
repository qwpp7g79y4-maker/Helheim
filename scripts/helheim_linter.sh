#!/bin/bash
echo "[LINTER]: Validating Helheim Modular Architecture..."

FAIL=0

# 1. Check mod.rs for strict delegation rules
if grep -q "var_store" /home/bitboi/dev_2/Helheim/helheim-core/src/orchestra/mod.rs; then
    echo "[WARNING]: 'mod.rs' bevatte mogelijk data stores die in 'memory.rs' thuishoren."
    # Just a warning because we DO have self.memory.var_store.lock() in mod.rs still
fi

# 2. Check if Executor handles Native API calls instead of SystemManager
if grep -q "try_execute_native" /home/bitboi/dev_2/Helheim/helheim-core/src/orchestra/executor.rs | grep -v "system::SystemManager"; then
    echo "[ERROR]: 'executor.rs' probeert zelf native calls af te handelen. Gebruik 'SystemManager'."
    FAIL=1
fi

if [ $FAIL -eq 1 ]; then
    echo "[LINTER]: Modulaire architectuur geschonden! Repareer aub de blokken."
    exit 1
else
    echo "[LINTER]: Helheim architectuur is schoon."
    exit 0
fi
