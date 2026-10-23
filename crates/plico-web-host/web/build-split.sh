#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
split_out=target/plico-web-host-split
mkdir -p "$split_out"
cargo build --release -p plico-web-host --features split --target wasm32-unknown-unknown
"${WASM_BINDGEN:-wasm-bindgen}" --target web --out-dir "$split_out/host" target/wasm32-unknown-unknown/release/plico_web_host.wasm
for variant in v1 v2 reset; do
  case "$variant" in
    v1) cargo build --release -p plico-web-demo --target wasm32-unknown-unknown ;;
    v2) cargo build --release -p plico-web-demo --features revised --target wasm32-unknown-unknown ;;
    reset) cargo build --release -p plico-web-demo --features revised,schema-reset --target wasm32-unknown-unknown ;;
  esac
  "${WASM_BINDGEN:-wasm-bindgen}" --target web --out-dir "$split_out/ui-$variant" target/wasm32-unknown-unknown/release/plico_web_demo.wasm
done
cp crates/plico-web-host/web/split.html "$split_out/index.html"
cp crates/plico-web-host/web/split.js "$split_out/"
cp backend/examples/web-preview/public/{DejaVuSans.ttf,FONT-LICENSE.txt} "$split_out/"
printf 'Serve with: python3 -m http.server 8081 --directory target/plico-web-host-split\n'
