#!/bin/sh

KINSHARE=/mnt/us/extensions/kinshare

pkill kinshare-client >> "$KINSHARE/logs.txt" 2>&1 || true

eips 3 3 "Kinshare stopped"
