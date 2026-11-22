#!/usr/bin/env bash
# Run from any directory on Linux with Xvfb, libX11 and libXtst development files.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --locked --release -p plico-nsh --features native --example vault_updates
target_dir=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
export replay_binary="$target_dir/release/examples/vault_updates"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cc -O2 benchmarks/nsh-vault/visible.c -o "$tmp/visible" -lX11 -lXtst
export GDK_BACKEND=x11 GSK_RENDERER=cairo GDK_SCALE=1
xvfb-run -a -s '-screen 0 1000x1100x24' bash -eu -c '
    for mode in tick-signal tick-diff tick-memo select-signal select-diff select-memo; do
        NSH_VAULT_WINDOW=$mode "$replay_binary" & pid=$!
        trap '\''kill "$pid" 2>/dev/null || true'\'' EXIT
        sleep 1
        # Bounds cover the two detail labels under each implementation’s font metrics.
        y=478
        case "$mode" in *signal) y=500;; esac
        "$1/visible" 20 9 0 "$y" 160 42 > "$1/$mode.ms"
        kill "$pid"
        wait "$pid" || true
        trap - EXIT
    done
' _ "$tmp"
python3 - "$tmp" <<'PY'
import pathlib, sys
print("mode,p50_ms,p95_ms")
for path in sorted(pathlib.Path(sys.argv[1]).glob("*.ms")):
    values = sorted(map(float, path.read_text().splitlines()))
    assert len(values) == 100
    print(f"{path.stem},{values[49]:.3f},{values[94]:.3f}")
PY
