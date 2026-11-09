#!/usr/bin/env bash
# Focused native verification, not a claim of live-window/ProMotion acceptance.
set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
    printf '%s\n' 'This check requires macOS; cross-compilation cannot run native tests.' >&2
    exit 2
fi
if [[ $# -ne 0 ]]; then
    printf '%s\n' 'This script takes no arguments. Set PLICO_MACOS_CHECK_REPORT_DIR to change the report directory.' >&2
    exit 2
fi
root_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root_dir"
report_dir=${PLICO_MACOS_CHECK_REPORT_DIR:-"$root_dir/reports/macos-native/$(date -u +%Y%m%dT%H%M%SZ)-$$"}
mkdir -p -- "$report_dir"
summary="$report_dir/summary.txt"
{
    printf 'Plico commit: %s\n' "$(git rev-parse HEAD)"
    if [[ -n $(git status --porcelain) ]]; then
        printf '%s\n' 'Worktree: dirty (results include local changes)'
        git status --short
    else
        printf '%s\n' 'Worktree: clean'
    fi
    printf 'macOS: %s (%s)\n' "$(sw_vers -productVersion)" "$(uname -m)"
    rustc --version
    cargo --version
    printf '%s\n' 'Scope: native text, retained raster/GPU reuse, shared host unit tests.'
    printf '%s\n' 'Not covered: live window resize, titlebar actions, ProMotion, OS-event queue identity, process RSS.'
} > "$summary"

run_tests() {
    local stage=$1
    shift
    local passed=0 line
    local pattern='^test result: ok[.] ([0-9]+) passed;'
    printf 'Running %s\n' "$stage"
    if "$@" 2>&1 | tee "$report_dir/$stage.log"; then
        while IFS= read -r line; do
            if [[ $line =~ $pattern ]]; then
                passed=${BASH_REMATCH[1]}
            fi
        done < "$report_dir/$stage.log"
    else
        printf 'FAIL %s (command failed)\n' "$stage" >> "$summary"
        return 1
    fi
    if [[ $passed -eq 0 ]]; then
        printf 'FAIL %s (no tests executed)\n' "$stage" | tee -a "$summary" >&2
        return 1
    fi
    printf 'PASS %s (%s tests)\n' "$stage" "$passed" >> "$summary"
}

run_tests coretext cargo test --color never --locked -p plico-native --no-default-features --features macos-text --lib macos_text:: -- --test-threads=1
run_tests vertical cargo test --color never --locked -p plico-native --no-default-features --features vertical-text --lib vertical:: -- --test-threads=1
run_tests bridge cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib macos_paragraph::tests:: -- --test-threads=1
run_tests native-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib macos_paragraph::gpu_tests:: -- --ignored --test-threads=1
run_tests raster-window-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib stable_native_raster_windows_preserve -- --ignored --test-threads=1
run_tests host cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib -- --test-threads=1
printf 'Native checks passed. Reports: %s\n' "$report_dir"
printf '%s\n' 'Live-window acceptance remains a separate check; see docs/macos-mygo.md.'
