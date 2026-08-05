#!/system/bin/sh
set -eu

MODDIR=${0%/*}
CTL="$MODDIR/bin/nethopctl"

"$CTL" config reload --wait
"$CTL" update --if-needed --wait || true
"$CTL" status
