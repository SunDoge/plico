#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
cargo build --release -p plico-web-host --features gallery --target wasm32-unknown-unknown
mkdir -p target/plico-web-host
"${WASM_BINDGEN:-wasm-bindgen}" --target web --out-dir target/plico-web-host/pkg   target/wasm32-unknown-unknown/release/plico_web_host.wasm
cp crates/plico-web-host/web/index.html crates/plico-web-host/web/main.js target/plico-web-host/
cp backend/examples/web-preview/public/DejaVuSans.ttf target/plico-web-host/
cp backend/examples/web-preview/public/FONT-LICENSE.txt target/plico-web-host/
printf 'Serve with: python3 -m http.server 8080 --directory target/plico-web-host\n'
