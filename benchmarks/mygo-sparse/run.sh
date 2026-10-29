#!/usr/bin/env bash
set -euo pipefail
: "${MYGO_DIR:?Set MYGO_DIR to a local MyGO checkout}"
fixture_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mygo_dir=$(cd -- "$MYGO_DIR" && pwd)
overlay_dir=$(mktemp -d)
trap 'rm -rf -- "$overlay_dir"' EXIT
jq -n --arg virtual "$mygo_dir/ui/zz_plico_sparse_test.go" --arg fixture "$fixture_dir/sparse_test.go" \
  '{Replace: {($virtual): $fixture}}' > "$overlay_dir/overlay.json"
printf 'MyGO commit: %s\n' "$(git -C "$mygo_dir" rev-parse HEAD)" >&2
cd -- "$mygo_dir"
run_pattern='^TestPlicoSparse(Table|Paragraphs)$'
bench_pattern="${BENCH_PATTERN:-^BenchmarkPlicoSparseTable$}"
if [[ ${MYGO_SPARSE_CPU:-} == 1 ]]; then
  run_pattern='^TestPlicoSparseCPU$'
  bench_pattern='^$'
fi
export HOST_CPUS="${HOST_CPUS:-$(getconf _NPROCESSORS_ONLN)}"
profile_args=()
if [[ -n ${CPU_PROFILE:-} ]]; then
  profile_args=(-cpuprofile "$CPU_PROFILE" -o "${CPU_PROFILE}.test")
fi
CGO_ENABLED=0 GOMAXPROCS="${GOMAXPROCS:-1}" go test -overlay "$overlay_dir/overlay.json" \
  -tags mygo_noinspector -run "$run_pattern" \
  -bench "$bench_pattern" \
  -benchtime "${BENCH_TIME:-200x}" -count "${BENCH_COUNT:-3}" "${profile_args[@]}" ./ui
