#!/usr/bin/env bash
# Focused native verification, including a short main-thread window exercise.
set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
    printf '%s\n' 'This check requires macOS; cross-compilation cannot run native tests.' >&2
    exit 2
fi
if [[ $# -ne 0 ]]; then
    printf '%s\n' 'This script takes no arguments. Set PLICO_MACOS_CHECK_REPORT_DIR to change the report directory.' >&2
    exit 2
fi
check_mode=${PLICO_MACOS_CHECK_MODE:-full}
if [[ $check_mode != full && $check_mode != cpu ]]; then
    printf '%s\n' 'PLICO_MACOS_CHECK_MODE must be full or cpu.' >&2
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
    printf 'Check mode: %s\n' "$check_mode"
    printf 'Async surface setting: %s\n' "${PLICO_WGPU_ASYNC_SURFACE:-default}"
    rustc --version
    cargo --version
    if [[ $check_mode == cpu ]]; then
        printf '%s\n' 'Scope: native CoreText geometry/raster, paragraph cache, worker QoS and CPU scheduling; no GPU or window checks.'
    else
        printf '%s\n' 'Scope: native text, raster/GPU reuse, shared host tests, opaque surface resize/idle/hide recovery and process memory samples.'
    fi
    printf '%s\n' 'Not covered: custom rounded/translucent window visuals, titlebar actions, ProMotion, OS-event queue identity, application scrolling/RSS growth.'
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
run_tests surface-worker cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib surface_acquire:: -- --test-threads=1
if [[ $check_mode == cpu ]]; then
    run_tests frame-pacing cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib frame_pacing:: -- --test-threads=1
    run_tests drawable-scheduling cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib drawable_wait_suspends -- --test-threads=1
    run_tests titlebar-policy cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib titlebar_action:: -- --test-threads=1
    printf 'Native CPU checks passed. Reports: %s\n' "$report_dir"
    printf '%s\n' 'GPU, live window and memory checks were not run; use full mode on a Mac with a working Metal display.'
    exit 0
fi
run_tests native-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib macos_paragraph::gpu_tests:: -- --ignored --test-threads=1
run_tests frame-planning-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib abandoned_prepared_frames_preserve -- --ignored --test-threads=1
run_tests raster-window-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib stable_native_raster_windows_preserve -- --ignored --test-threads=1
run_tests host cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib -- --test-threads=1
run_surface_window() {
    local marker='' line phase memory_phases=' '
    local memory_pattern='^surface-memory: phase=([^[:space:]]+) rss_bytes=([0-9]+) footprint_bytes=([0-9]+)$'
    printf '%s\n' 'Running live-window (opens a window briefly; keep it visible)'
    if cargo run --color never --locked -p plico-wgpu --no-default-features --features window,system-text --example surface-lifecycle 2>&1 | tee "$report_dir/live-window.log"; then
        while IFS= read -r line; do
            if [[ $line == 'surface-lifecycle: PASS '* ]]; then marker=$line; fi
            if [[ $line == 'surface-adapter: '* ]]; then printf '%s\n' "$line" >> "$summary"; fi
            if [[ $line =~ $memory_pattern ]]; then
                phase=${BASH_REMATCH[1]}
                memory_phases+="$phase "
                printf '%s\n' "$line" >> "$summary"
            fi
        done < "$report_dir/live-window.log"
    else
        printf '%s\n' 'FAIL live-window (command failed)' >> "$summary"
        return 1
    fi
    if [[ -z $marker ]]; then
        printf '%s\n' 'FAIL live-window (completion marker missing)' | tee -a "$summary" >&2
        return 1
    fi
    for phase in startup initial warmup resized idle-trimmed idle-recovered hidden-trimmed hide-recovered; do
        if [[ $memory_phases != *" $phase "* ]]; then
            printf 'FAIL live-window (missing memory sample: %s)\n' "$phase" | tee -a "$summary" >&2
            return 1
        fi
    done
    printf '%s\n' "PASS live-window: $marker" >> "$summary"
}
run_surface_window
printf 'Native checks passed. Reports: %s\n' "$report_dir"
printf '%s\n' 'Custom window visuals, OS input, ProMotion and application RSS still require separate acceptance; see docs/macos-mygo.md.'
