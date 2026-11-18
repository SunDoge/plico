#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
tools="${ANDROID_HOME:?}/build-tools/35.0.0"
platform="$ANDROID_HOME/platforms/android-35/android.jar"
cargo apk build --manifest-path apps/android-counter/Cargo.toml --lib
apk="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')/debug/apk"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/classes"
javac --release 8 -classpath "$platform" -d "$tmp/classes" crates/plico-android/java/dev/plico/PlicoActivity.java
jar cf "$tmp/bridge.jar" -C "$tmp/classes" .
"$tools/d8" --min-api 28 --lib "$platform" --output "$tmp" "$tmp/bridge.jar"
cp "$apk/android-counter-unaligned.apk" "$tmp/native.apk"
(cd "$tmp" && zip -q -0 native.apk classes.dex)
"$tools/zipalign" -P 16 -f 4 "$tmp/native.apk" "$tmp/aligned.apk"
"$tools/apksigner" sign --ks "${ANDROID_USER_HOME:-$HOME/.android}/debug.keystore" --ks-pass pass:android --out "$tmp/android-counter.apk" "$tmp/aligned.apk"
"$tools/apksigner" verify "$tmp/android-counter.apk"
mv "$tmp/android-counter.apk" "$apk/"
mv "$tmp/android-counter.apk.idsig" "$apk/"
