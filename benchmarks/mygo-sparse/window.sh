#!/usr/bin/env bash
set -euo pipefail
: "${MYGO_DIR:?Set MYGO_DIR to a local MyGO checkout}"
fixture_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mygo_dir=$(cd -- "$MYGO_DIR" && pwd)
output_dir=${SPARSE_OUTPUT_DIR:-"$fixture_dir/../../reports/mygo-sparse"}
mkdir -p -- "$output_dir"
output_dir=$(cd -- "$output_dir" && pwd)
overlay_dir=$(mktemp -d)
trap 'rm -rf -- "$overlay_dir"' EXIT
jq -n --arg main "$mygo_dir/examples/counter-native/main.go" --arg src "$fixture_dir/window.go" \
 --arg hook "$mygo_dir/ui/zz_plico_window_probe_linux.go" --arg probe "$fixture_dir/window_probe_linux.go" \
 '{Replace:{($main):$src,($hook):$probe}}' > "$overlay_dir/overlay.json"
export HOST_CPUS="${HOST_CPUS:-$(getconf _NPROCESSORS_ONLN)}"
cd -- "$mygo_dir"
CGO_ENABLED=0 go build -overlay "$overlay_dir/overlay.json" -tags mygo_noinspector \
 -o "$output_dir/mygo-sparse-window" ./examples/counter-native
if [[ ${SPARSE_BUILD_ONLY:-} != 1 ]]; then "$output_dir/mygo-sparse-window"; fi
