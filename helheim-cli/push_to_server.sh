#!/bin/bash
set -e

# Configuration
REMOTE_USER="bitboi_slave"
REMOTE_HOST="$1" # Pass IP as argument, e.g. ./push_to_server.sh 192.168.1.50

if [ -z "$REMOTE_HOST" ]; then
    echo "❌ Gebruik: ./push_to_server.sh [SERVER_IP]"
    echo "   Voorbeeld: ./push_to_server.sh 192.168.1.200"
    exit 1
fi

echo "🚀 [DEPLOY]: Starten van Helheim Hive Deployment naar $REMOTE_USER@$REMOTE_HOST..."

# 1. Sync Helheim CLI (Source Code)
echo "📦 [SYNC]: Helheim CLI..."
rsync -avz --exclude 'target' --exclude '.git' \
    /media/bitboi/DATA1/Helheim/helheim-cli/ \
    $REMOTE_USER@$REMOTE_HOST:~/Helheim/helheim-cli/

# 2. Sync Helheim Brain (Source Code)
echo "🧠 [SYNC]: Helheim Brain..."
rsync -avz --exclude 'target' --exclude '.git' \
    /media/bitboi/DATA1/Helheim_Brain/ \
    $REMOTE_USER@$REMOTE_HOST:~/Helheim_Brain/

# 3. Trigger Remote Build & Install
echo "🛠️  [BUILD]: Uitvoeren van install_hive.sh op server..."
ssh -t $REMOTE_USER@$REMOTE_HOST "bash ~/Helheim/helheim-cli/install_hive.sh"

echo "✅ [DONE]: Server $REMOTE_HOST is bijgewerkt met de nieuwste Helheim Core."
