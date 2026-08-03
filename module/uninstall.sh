#!/system/bin/sh
set -eu

MODDIR=${0%/*}
DATA_ROOT=/data/adb/nethop
PID_FILE="$DATA_ROOT/run/supervisor.pid"
CTL="$MODDIR/bin/nethopctl"

if [ -x "$CTL" ]; then
  "$CTL" stop >/dev/null 2>&1 || :
fi

if [ -f "$PID_FILE" ] && [ ! -L "$PID_FILE" ]; then
  read -r pid expected_start extra < "$PID_FILE" || true
  valid_identity=true
  case "${pid:-}" in ''|*[!0-9]*) valid_identity=false ;; esac
  case "${expected_start:-}" in ''|*[!0-9]*) valid_identity=false ;; esac
  [ -z "${extra:-}" ] || valid_identity=false
  if [ "$valid_identity" = true ]; then
      stat_file="/proc/$pid/stat"
      cmdline_file="/proc/$pid/cmdline"
      if [ -r "$stat_file" ] && [ -r "$cmdline_file" ]; then
        stat_line=$(cat "$stat_file")
        stat_fields=${stat_line##*) }
        actual_start=$(printf '%s\n' "$stat_fields" | awk '{print $20}')
        cmdline=$(tr '\000' ' ' < "$cmdline_file")
        case "$cmdline" in
          *"$MODDIR/bin/nethopd --supervise --root $DATA_ROOT"*)
            if [ "$actual_start" = "$expected_start" ]; then
              kill -TERM "$pid" 2>/dev/null || :
              attempts=0
              while kill -0 "$pid" 2>/dev/null && [ "$attempts" -lt 5 ]; do
                sleep 1
                attempts=$((attempts + 1))
              done
              if kill -0 "$pid" 2>/dev/null; then
                echo "NetHop supervisor did not stop; preserving persistent data" >&2
                exit 1
              fi
            fi
            ;;
        esac
      fi
  fi
fi

rm -rf "$DATA_ROOT"
