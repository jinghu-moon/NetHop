#!/system/bin/sh
set -eu

MODDIR=${0%/*}
exec "$MODDIR/bin/nethopd" --supervise --root /data/adb/nethop

