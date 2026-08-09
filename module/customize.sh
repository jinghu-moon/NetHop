#!/system/bin/sh

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

publish_persistent_asset() {
  relative=$1
  destination=$2
  source="$MODPATH/$relative"
  temporary="${destination}.new"

  if [ -e "$destination" ] || [ -L "$destination" ]; then
    [ -f "$destination" ] && [ ! -L "$destination" ] || fail "persistent asset target is invalid: ${destination##*/}"
  fi
  if [ -e "$temporary" ] || [ -L "$temporary" ]; then
    [ -f "$temporary" ] && [ ! -L "$temporary" ] || fail "persistent asset staging path is invalid: ${temporary##*/}"
    rm -f "$temporary" || fail "could not clear stale persistent asset staging file"
  fi

  cp "$source" "$temporary" || fail "could not stage persistent asset: ${destination##*/}"
  chown 0:0 "$temporary" || fail "could not secure persistent asset: ${destination##*/}"
  chmod 0600 "$temporary" || fail "could not secure persistent asset: ${destination##*/}"
  expected=$(expected_digest "$relative")
  actual=$(actual_digest "$temporary")
  if [ "$actual" != "$expected" ]; then
    rm -f "$temporary"
    fail "persistent asset checksum mismatch: ${destination##*/}"
  fi
  mv -f "$temporary" "$destination" || fail "could not publish persistent asset: ${destination##*/}"
}

[ "${API:-0}" -ge 33 ] || fail "Android API 33 or later is required"
[ "${ARCH:-}" = "arm64" ] || fail "only arm64 is supported"
if [ -z "${MAGISK_VER_CODE:-}" ] && [ -z "${KSU_VER_CODE:-}" ] && [ "${KSU:-false}" != "true" ]; then
  fail "Magisk or KernelSU installation environment is required"
fi

require_regular_file "$CHECKSUMS"
checksum_count=$(wc -l < "$CHECKSUMS" | tr -d ' ')
[ "$checksum_count" -ge 7 ] || fail "checksum manifest is incomplete"
while read -r digest relative extra; do
  [ -n "$digest" ] && [ -n "$relative" ] && [ -z "$extra" ] || fail "invalid checksum entry"
  case "$relative" in
    bin/nethopd|bin/nethopctl|bin/sing-box|rulesets/cn-domain.srs|rulesets/cn-ip.srs|build-manifest.json|licenses/webui-sbom.cdx.json|licenses/webui-licenses.json|licenses/webui-production-bundle.json|licenses/webui-bundle-metafile.json|webroot/index.html|webroot/.vite/manifest.json|webroot/assets/*)
      verify_asset "$relative"
      ;;
    *) fail "unexpected checksum target: $relative" ;;
  esac
done < "$CHECKSUMS"

for directory in \
  "$DATA_ROOT" \
  "$DATA_ROOT/config" \
  "$DATA_ROOT/generations" \
  "$DATA_ROOT/subscriptions/cache" \
  "$DATA_ROOT/subscriptions/reports" \
  "$DATA_ROOT/rulesets" \
  "$DATA_ROOT/stats" \
  "$DATA_ROOT/state" \
  "$DATA_ROOT/state/ruleset-cache" \
  "$DATA_ROOT/run" \
  "$DATA_ROOT/logs"
do
  [ ! -L "$directory" ] || fail "persistent path must not be a symlink"
  mkdir -p "$directory" || fail "could not create persistent directory"
  chown 0:0 "$directory"
  chmod 0700 "$directory"
done

publish_persistent_asset "rulesets/cn-domain.srs" "$DATA_ROOT/rulesets/cn-domain.srs"
publish_persistent_asset "rulesets/cn-ip.srs" "$DATA_ROOT/rulesets/cn-ip.srs"

require_regular_file "$MODPATH/defaults/nethop.toml"
if [ ! -e "$DATA_ROOT/config/nethop.toml" ]; then
  cp "$MODPATH/defaults/nethop.toml" "$DATA_ROOT/config/nethop.toml"
elif [ -L "$DATA_ROOT/config/nethop.toml" ] || [ ! -f "$DATA_ROOT/config/nethop.toml" ]; then
  fail "existing managed config is not a regular file"
elif ! grep -Eq '^[[:space:]]*schema_version[[:space:]]*=[[:space:]]*2[[:space:]]*$' "$DATA_ROOT/config/nethop.toml"; then
  if [ ! -e "$DATA_ROOT/config/nethop.toml.pre-v2" ]; then
    cp "$DATA_ROOT/config/nethop.toml" "$DATA_ROOT/config/nethop.toml.pre-v2"
    chown 0:0 "$DATA_ROOT/config/nethop.toml.pre-v2"
    chmod 0600 "$DATA_ROOT/config/nethop.toml.pre-v2"
  fi
  cp "$MODPATH/defaults/nethop.toml" "$DATA_ROOT/config/nethop.toml"
fi
chown 0:0 "$DATA_ROOT/config/nethop.toml"
chmod 0600 "$DATA_ROOT/config/nethop.toml"

if [ -L "$MODPATH/config" ] || { [ -e "$MODPATH/config" ] && [ ! -d "$MODPATH/config" ]; }; then
  fail "module config entry is invalid"
fi
mkdir -p "$MODPATH/config" || fail "could not create module config directory"
if [ -e "$MODPATH/config/nethop.toml" ] || [ -L "$MODPATH/config/nethop.toml" ]; then
  [ -L "$MODPATH/config/nethop.toml" ] || fail "module config entry is not a symlink"
  [ "$(readlink "$MODPATH/config/nethop.toml")" = "$DATA_ROOT/config/nethop.toml" ] || fail "module config symlink target is invalid"
else
  ln -s "$DATA_ROOT/config/nethop.toml" "$MODPATH/config/nethop.toml" || fail "could not publish module config link"
fi

set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/action.sh" 0 0 0755
set_perm "$MODPATH/uninstall.sh" 0 0 0755
set_perm "$MODPATH/bin/nethopd" 0 0 0755
set_perm "$MODPATH/bin/nethopctl" 0 0 0755
set_perm "$MODPATH/bin/sing-box" 0 0 0755
set_perm "$MODPATH/rulesets/cn-domain.srs" 0 0 0644
set_perm "$MODPATH/rulesets/cn-ip.srs" 0 0 0644
set_perm "$MODPATH/defaults/nethop.toml" 0 0 0644
set_perm "$BUILD_MANIFEST" 0 0 0644
set_perm "$CHECKSUMS" 0 0 0644

ui_print "- NetHop package integrity and persistent layout verified"
