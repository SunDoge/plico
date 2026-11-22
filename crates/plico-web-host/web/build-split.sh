#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
target_dir=${CARGO_TARGET_DIR:-target}
split_out="$target_dir/plico-web-host-split"
release="$target_dir/wasm32-unknown-unknown/release"
mkdir -p "$split_out"
cargo build --locked --release -p plico-web-host --features split --target wasm32-unknown-unknown
"${WASM_BINDGEN:-wasm-bindgen}" --target web --out-dir "$split_out/host" "$release/plico_web_host.wasm"
variants=(v1 v2 reset)
if [[ ${1:-} == --dev ]]; then variants=(); fi
for variant in "${variants[@]}"; do
  case "$variant" in
    v1) features=guest ;;
    v2) features=guest,revised ;;
    reset) features=guest,revised,schema-reset ;;
  esac
  cargo rustc --locked --release -p web-demo --lib --features "$features" --target wasm32-unknown-unknown -- -C strip=symbols
  cp "$release/web_demo.wasm" "$split_out/ui-$variant.wasm"
done
cp crates/plico-web-host/web/split.html "$split_out/index.html"
cp crates/plico-wasm/web/module.mjs "$split_out/"
cp crates/plico-web-host/web/split.js crates/plico-web-host/web/input.js crates/plico-web-host/web/frames.js "$split_out/"
if [[ ${1:-} == --dev ]]; then
  exec python3 crates/plico-web-host/web/dev-split.py
fi
printf 'Serve with: python3 -m http.server 8081 --directory %q\n' "$split_out"
