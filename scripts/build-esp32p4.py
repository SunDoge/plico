#!/usr/bin/env python3
"""Build the Plico staticlib against an already configured IDF LVGL component.

PLICO_IDF_BUILD must contain compile_commands.json; PLICO_CC must name the
matching riscv32-esp-elf-gcc. Never modifies the supplied IDF project or flashes.
"""
import json
import hashlib
import io
import tarfile
import urllib.request
import os
from pathlib import Path
import shlex
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
STAGE = Path(os.environ.get("PLICO_P4_STAGE", ROOT / "target/esp32p4"))
SYS_VERSION = "9.5.4"
TOOLCHAIN = "nightly-2026-10-15"


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists() or path.read_text() != text:
        path.write_text(text)


def main():
    build = Path(os.environ["PLICO_IDF_BUILD"]).resolve()
    cc = Path(os.environ["PLICO_CC"]).resolve()
    commands = json.loads((build / "compile_commands.json").read_text())
    entry = next(c for c in commands if c["file"].endswith("/lv_obj.c"))
    flags = entry.get("arguments") or shlex.split(entry["command"])
    directory = Path(entry["directory"])
    args = ["--target=riscv32-unknown-elf", "-march=rv32imafc", "-mabi=ilp32f"]
    # GCC compilation databases from IDF use joined -I/-D options.
    for flag in flags:
        if flag.startswith("-I"):
            args.append("-I" + str((directory / flag[2:]).resolve()))
        elif flag.startswith("-D"):
            args.append(flag)
    sysroot = subprocess.check_output([str(cc), "-print-sysroot"], text=True).strip()
    args += [f"--sysroot={sysroot}", f"-I{sysroot}/include"]
    args_file = STAGE / "clang-args.txt"
    write(args_file, "\n".join(args) + "\n")

    # Verify the registry archive using the workspace lockfile. Extract only
    # the Rust crate: IDF supplies C headers/sources, so no vendored LVGL copy.
    name = f"lightvgl-sys-{SYS_VERSION}"
    package = next(p for p in tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
                   if p["name"] == "lightvgl-sys" and p["version"] == SYS_VERSION)
    archives = list((Path.home() / ".cargo/registry/cache").glob(f"*/{name}.crate"))
    data = archives[0].read_bytes() if archives else urllib.request.urlopen(
        f"https://static.crates.io/crates/lightvgl-sys/{name}.crate", timeout=60).read()
    if hashlib.sha256(data).hexdigest() != package["checksum"]:
        raise SystemExit("lightvgl-sys archive checksum mismatch")
    syscrate = STAGE / name
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive.getmembers():
            relative = Path(member.name).relative_to(name)
            if member.isfile() and (str(relative) == "Cargo.toml" or relative.parts[0] == "src"):
                write(syscrate / relative, archive.extractfile(member).read().decode())
    manifest = (syscrate / "Cargo.toml").read_text().replace(
        '[build-dependencies.bindgen]\nversion = ">=0.71"',
        '[build-dependencies.bindgen]\nversion = ">=0.71"\nfeatures = ["experimental"]')
    write(syscrate / "Cargo.toml", manifest)
    write(syscrate / "build.rs", (ROOT / "crates/plico-lvgl/esp-idf/lightvgl-build.rs").read_text())

    # A separate Cargo root keeps patched embedded dependencies out of the main
    # lockfile. Source files and Plico crates still come from this repository.
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    source = Path(os.environ.get("PLICO_P4_RUST_SOURCE", ROOT / "apps/lvgl-esp32p4/src/lib.rs")).resolve()
    manifest = f'''[package]
name = "lvgl-esp32p4"
version = "{version}"
edition = "2024"
[workspace]
[lib]
path = {json.dumps(str(source))}
crate-type = ["staticlib"]
[features]
default = ["firmware"]
firmware = ["plico-lvgl/external"]
[dependencies]
plico-core = {{ path = {json.dumps(str(ROOT / "crates/plico-core"))} }}
plico-lvgl = {{ path = {json.dumps(str(ROOT / "crates/plico-lvgl"))} }}
[patch.crates-io]
lightvgl-sys = {{ path = {json.dumps(str(syscrate))} }}
[profile.release]
opt-level = "s"
panic = "abort"
lto = "fat"
codegen-units = 1
'''
    migrated = not (STAGE / "Cargo.toml").exists() or (STAGE / "Cargo.toml").read_text() != manifest
    write(STAGE / "Cargo.toml", manifest)
    env = os.environ.copy()
    env.update(PLICO_LVGL_CLANG_ARGS=str(args_file), PLICO_CC=str(cc),
               CARGO_TARGET_DIR=str(STAGE / "build"), RUSTFLAGS="--cfg espidf_time64")
    # Use the actual rustup toolchain executables, bypassing local cache wrappers.
    cargo = subprocess.check_output(["rustup", "which", "--toolchain", TOOLCHAIN, "cargo"], text=True).strip()
    env["RUSTC"] = subprocess.check_output(["rustup", "which", "--toolchain", TOOLCHAIN, "rustc"], text=True).strip()
    command = [cargo, "build", "--release", "--target", "riscv32imafc-esp-espidf",
               "-Zbuild-std=std,panic_abort"]
    # Regenerate only the isolated lockfile when migrating binding backends.
    if migrated:
        run([cargo, "generate-lockfile"], cwd=STAGE, env=env)
    # First resolution creates the isolated lockfile; subsequent builds lock it.
    if (STAGE / "Cargo.lock").exists():
        command.append("--locked")
    run(command, cwd=STAGE, env=env)
    print(STAGE / "build/riscv32imafc-esp-espidf/release/liblvgl_esp32p4.a")


if __name__ == "__main__":
    main()
