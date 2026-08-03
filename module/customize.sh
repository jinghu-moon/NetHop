#!/system/bin/sh
set -eu

DATA_ROOT=/data/adb/nethop
CHECKSUMS="$MODPATH/checksums.sha256"
BUILD_MANIFEST="$MODPATH/build-manifest.json"

fail() {
  abort "! NetHop: $1"
}

require_regular_file() {
  [ -f "$1" ] && [ ! -L "$1" ] || fail "invalid package file: ${1##*/}"
}

expected_digest() {
  file_name=$1
  digest=$(awk -v name="$file_name" '
    $2 == name && length($1) == 64 && $1 !~ /[^0-9A-Fa-f]/ { value=tolower($1); count++ }
    END { if (count == 1) print value }
  ' "$CHECKSUMS")
  [ -n "$digest" ] || fail "missing or duplicate checksum: $file_name"
  printf '%s\n' "$digest"
}

actual_digest() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print tolower($1)}'
  else
    toybox sha256sum "$1" | awk '{print tolower($1)}'
  fi
}

verify_asset() {
  relative=$1
  path="$MODPATH/$relative"
  require_regular_file "$path"
  expected=$(expected_digest "$relative")
  actual=$(actual_digest "$path")
  [ "$actual" = "$expected" ] || fail "checksum mismatch: $relative"
}

[ "${API:-0}" -ge 33 ] || fail "Android API 33 or later is required"
[ "${ARCH:-}" = "arm64" ] || fail "only arm64 is supported"
if [ -z "${MAGISK_VER_CODE:-}" ] && [ -z "${KSU_VER_CODE:-}" ] && [ "${KSU:-false}" != "true" ]; then
  fail "Magisk or KernelSU installation environment is required"
fi

require_regular_file "$CHECKSUMS"
[ "$(wc -l < "$CHECKSUMS" | tr -d ' ')" -eq 4 ] || fail "checksum manifest must contain four entries"
verify_asset "bin/nethopd"
verify_asset "bin/nethopctl"
verify_asset "bin/sing-box"
verify_asset "build-manifest.json"

for directory in \
  "$DATA_ROOT" \
  "$DATA_ROOT/config" \
  "$DATA_ROOT/generations" \
  "$DATA_ROOT/subscriptions/cache" \
  "$DATA_ROOT/subscriptions/reports" \
  "$DATA_ROOT/rulesets" \
  "$DATA_ROOT/stats" \
  "$DATA_ROOT/state" \
  "$DATA_ROOT/run" \
  "$DATA_ROOT/logs"
do
  [ ! -L "$directory" ] || fail "persistent path must not be a symlink"
  mkdir -p "$directory" || fail "could not create persistent directory"
  chown 0:0 "$directory"
  chmod 0700 "$directory"
done

if [ ! -e "$DATA_ROOT/config/nethop.json" ]; then
  require_regular_file "$MODPATH/defaults/nethop.json"
  cp "$MODPATH/defaults/nethop.json" "$DATA_ROOT/config/nethop.json"
elif [ -L "$DATA_ROOT/config/nethop.json" ] || [ ! -f "$DATA_ROOT/config/nethop.json" ]; then
  fail "existing managed config is not a regular file"
fi
chown 0:0 "$DATA_ROOT/config/nethop.json"
chmod 0600 "$DATA_ROOT/config/nethop.json"

set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/action.sh" 0 0 0755
set_perm "$MODPATH/uninstall.sh" 0 0 0755
set_perm "$MODPATH/bin/nethopd" 0 0 0755
set_perm "$MODPATH/bin/nethopctl" 0 0 0755
set_perm "$MODPATH/bin/sing-box" 0 0 0755
set_perm "$BUILD_MANIFEST" 0 0 0644
set_perm "$CHECKSUMS" 0 0 0644

ui_print "- NetHop package integrity and persistent layout verified"

