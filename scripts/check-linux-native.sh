#!/usr/bin/env bash
# Focused native verification, including a short main-thread window exercise.
set -euo pipefail

if [[ $(uname -s) != Linux ]]; then
    printf '%s\n' 'This check requires Linux and a working display/GPU. For headless X11 use xvfb-run -a.' >&2
    exit 2
fi
if [[ $# -ne 0 ]]; then
    printf '%s\n' 'This script takes no arguments. Set PLICO_LINUX_CHECK_REPORT_DIR to change the report directory.' >&2
    exit 2
fi
# X11-only regression fixtures need a server; do not force the live window off Wayland.
x11_runner=()
if [[ -z ${DISPLAY:-} ]]; then
    if ! command -v xvfb-run >/dev/null; then
        printf '%s\n' 'X11 fixtures require DISPLAY or xvfb-run.' >&2
        exit 2
    fi
    x11_runner=(xvfb-run -a)
fi
root_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root_dir"
report_dir=${PLICO_LINUX_CHECK_REPORT_DIR:-"$root_dir/reports/linux-native/$(date -u +%Y%m%dT%H%M%SZ)-$$"}
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
    uname -srmo
    printf 'Display: DISPLAY=%s WAYLAND_DISPLAY=%s XDG_SESSION_TYPE=%s\n' "${DISPLAY:-unset}" "${WAYLAND_DISPLAY:-unset}" "${XDG_SESSION_TYPE:-unset}"
    printf 'Async surface setting: %s\n' "${PLICO_WGPU_ASYNC_SURFACE:-default}"
    rustc --version
    cargo --version
    printf '%s\n' 'Scope: portal recovery, shared host, X11 resize, GPU damage/idle recovery and selected-backend opaque window lifecycle.'
    printf '%s\n' 'Not covered: real high-refresh pacing, fractional scaling, compositor occlusion, input latency, process RSS or GNOME/KDE settings-panel integration.'
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

run_tests host cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib -- --test-threads=1
run_tests portal dbus-run-session -- cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib linux_preferences -- --ignored --test-threads=1
# These two fixtures deliberately use X11 even when the live-window stage uses Wayland.
run_tests x11-host "${x11_runner[@]}" cargo test --color never --locked -p plico-shell --no-default-features --features wgpu --lib native_resize_uses_latest_extent -- --ignored --test-threads=1
run_tests x11-surface "${x11_runner[@]}" cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib native_surface_resize_waits -- --ignored --test-threads=1
run_tests frame-planning-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib abandoned_prepared_frames_preserve -- --ignored --test-threads=1
run_tests raster-window-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib stable_native_raster_windows_preserve -- --ignored --test-threads=1
run_tests layer-owners-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib encoded_layers_release_scene -- --ignored --test-threads=1
run_tests idle-target-gpu cargo test --color never --locked -p plico-wgpu --no-default-features --features window,system-text --lib trimming_idle_target -- --ignored --test-threads=1
run_surface_window() {
    local marker='' line
    printf '%s\n' 'Running live-window (opens a window briefly; keep it visible)'
    if cargo run --color never --locked -p plico-wgpu --no-default-features --features window,system-text --example surface-lifecycle 2>&1 | tee "$report_dir/live-window.log"; then
        while IFS= read -r line; do
            if [[ $line == 'surface-lifecycle: PASS '* ]]; then marker=$line; fi
        done < "$report_dir/live-window.log"
    else
        printf '%s\n' 'FAIL live-window (command failed)' >> "$summary"
        return 1
    fi
    if [[ -z $marker ]]; then
        printf '%s\n' 'FAIL live-window (completion marker missing)' | tee -a "$summary" >&2
        return 1
    fi
    printf '%s\n' "PASS live-window: $marker" >> "$summary"
}
run_surface_window
printf 'Native checks passed. Reports: %s\n' "$report_dir"
printf '%s\n' 'Real Wayland/high-refresh, fractional scaling, OS input and RSS still require separate acceptance; see docs/linux-mygo.md.'
