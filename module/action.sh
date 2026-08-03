#!/system/bin/sh
set -eu

MODDIR=${0%/*}
CTL="$MODDIR/bin/nethopctl"

status=$("$CTL" status)
if printf '%s\n' "$status" | grep -q '"state":"running_'; then
  "$CTL" stop
else
  "$CTL" start
fi
"$CTL" status
