#!/bin/sh
# Run after cargo build -p plico-cli.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/app/src" "$tmp/app/assets"
cat > "$tmp/app/Cargo.toml" <<'TOML'
[package]
name = "bundle-smoke"
version = "0.1.0"
edition = "2024"
[workspace]
[package.metadata.plico.bundle]
resources = ["assets"]
TOML
cat > "$tmp/app/src/main.rs" <<'RS'
fn main() {
    let dir = std::env::var("PLICO_RESOURCE_DIR").unwrap();
    assert_eq!(std::fs::read_to_string(format!("{dir}/assets/message.txt")).unwrap(), "bundled\n");
    assert_eq!(std::env::args().nth(1).as_deref(), Some("hello world"));
    println!("bundle smoke passed");
}
RS
printf 'bundled\n' > "$tmp/app/assets/message.txt"
cargo generate-lockfile --manifest-path "$tmp/app/Cargo.toml" --offline
"$root/target/debug/plico" bundle "$tmp/app" --output "$tmp/output with spaces"
(cd / && "$tmp/output with spaces/launch" 'hello world')
if "$root/target/debug/plico" bundle "$tmp/app" --output "$tmp/output with spaces"; then
    echo 'existing destination was overwritten' >&2
    exit 1
fi
