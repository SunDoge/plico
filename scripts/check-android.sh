#!/usr/bin/env bash
set -euo pipefail

cargo test --locked --profile ci -p plico-platform
for target in aarch64-linux-android x86_64-linux-android; do
    cargo check --locked --profile ci --target "$target" -p android-counter
    packages=$(cargo tree --locked --target "$target" -p android-counter -e normal --prefix none --format '{p}')
    if grep -E '^(plico-desktop|plico-shell|arboard|tray-icon|rfd) ' <<<"$packages"; then
        echo "Android host depends on a desktop package" >&2
        exit 1
    fi
    surface=$(cargo tree --locked --target "$target" -p plico-wgpu --no-default-features --features surface -e normal --prefix none --format '{p}')
    if grep -E '^winit ' <<<"$surface"; then
        echo "Surface renderer depends on a window host" >&2
        exit 1
    fi
done
