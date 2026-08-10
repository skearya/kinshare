#!/bin/sh

KINSHARE=/mnt/us/extensions/kinshare

pkill kinshare-client >> "$KINSHARE/logs.txt" 2>&1 || true

eips 1 1 "Kinshare stopped"
