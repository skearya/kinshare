#!/bin/sh

KINSHARE=/mnt/us/extensions/kinshare

pkill kinshare-client 2>/dev/null || true

nohup "$KINSHARE/bin/kinshare-client" >> "$KINSHARE/logs.txt" 2>&1 &
PID=$!

if kill -0 "$PID" 2>/dev/null; then
    eips 3 3 "Kinshare started"
else
    eips 3 3 "Kinshare failed to start, check log"
fi
