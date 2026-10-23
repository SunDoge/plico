#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
cargo build --release -p plico-web --features gallery --target wasm32-unknown-unknown
mkdir -p target/plico-web
"${WASM_BINDGEN:-wasm-bindgen}" --target web --out-dir target/plico-web/pkg   target/wasm32-unknown-unknown/release/plico_web.wasm
cp crates/plico-web/web/index.html crates/plico-web/web/main.js target/plico-web/
cp backend/examples/web-preview/public/DejaVuSans.ttf target/plico-web/
cp backend/examples/web-preview/public/FONT-LICENSE.txt target/plico-web/
printf 'Serve with: python3 -m http.server 8080 --directory target/plico-web\n'
