#!/bin/bash

if [ "$1" != "run" ]; then
    echo "Gebruik: ./helheim-run.sh run \"jouw command\""
    exit 1
fi

COMMAND="$2"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
LOG_FILE="helheim-logs/helheim-log-latest.md"

# Zorg dat log file bestaat
if [ ! -f "$LOG_FILE" ]; then
    echo "# Helheim Logboek – Automatisch bijhouden" > "$LOG_FILE"
    echo "" >> "$LOG_FILE"
fi

# Run Helheim en vang output
echo "Helheim runt: $COMMAND"
cargo run -- run "$COMMAND" | tee /tmp/helheim-output.txt

# Update log
echo "" >> "$LOG_FILE"
echo "## Auto-update op $TIMESTAMP" >> "$LOG_FILE"
echo "Command: run \"$COMMAND\"" >> "$LOG_FILE"
echo "Output:" >> "$LOG_FILE"
echo "\`\`\`" >> "$LOG_FILE"
cat /tmp/helheim-output.txt >> "$LOG_FILE"
echo "\`\`\`" >> "$LOG_FILE"

# Git diff als git aanwezig
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "Git diff:" >> "$LOG_FILE"
    git diff --stat HEAD^ HEAD 2>/dev/null || echo "Geen vorige commit" >> "$LOG_FILE"
else
    echo "Geen git repo – alleen handmatige log" >> "$LOG_FILE"
fi

echo "" >> "$LOG_FILE"

echo "Log automatisch bijgewerkt: $LOG_FILE"
rm -f /tmp/helheim-output.txt
