#!/usr/bin/env python3
"""Build the Plico staticlib against an already configured IDF LVGL component.

PLICO_IDF_BUILD must contain compile_commands.json; PLICO_CC must name the
matching riscv32-esp-elf-gcc. Never modifies the supplied IDF project or flashes.
"""
import json
import os
from pathlib import Path
import shlex
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
STAGE = ROOT / "target/esp32p4"
REV = "ae2328976dab755bd29d97f45b3e8c4253b9adc2"
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

    # Isolated, pinned dependency copy: no patches to the desktop workspace or
    # to the caller's firmware. Only compile wrappers; IDF supplies LVGL itself.
    oxi = STAGE / "oxivgl"
    if not oxi.exists():
        run(["git", "clone", "--no-checkout", "https://github.com/emobotics-dev/oxivgl.git", str(oxi)])
        run(["git", "checkout", "--detach", REV], cwd=oxi)
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=oxi, text=True).strip()
    if actual != REV:
        raise SystemExit(f"Unexpected oxivgl revision: {actual}")

    def original(path):
        return subprocess.check_output(["git", "show", f"{REV}:{path}"], cwd=oxi, text=True)

    overlay = ROOT / "crates/plico-lvgl/esp-idf"
    write(oxi / "oxivgl-sys/build.rs", (overlay / "build.rs").read_text())
    write(oxi / "oxivgl-sys/src/lib.rs", original("oxivgl-sys/src/lib.rs").replace(
        "#![no_std]", "#![no_std]\n#![allow(unnecessary_transmutes)]"))
    marker = "    // Image assets (all targets)"
    oxi_build = original("build.rs")
    if oxi_build.count(marker) != 1:
        raise SystemExit("oxivgl asset build adapter needs review")
    write(oxi / "build.rs", oxi_build.split(marker)[0] + "}\n")
    driver = original("src/driver.rs").replace(
        'not(target_os = "none")', 'not(any(target_os = "none", target_os = "espidf"))')
    write(oxi / "src/driver.rs", driver + "\n" + (overlay / "driver.rs").read_text())

    # A separate Cargo root keeps patched embedded dependencies out of the main
    # lockfile. Source files and Plico crates still come from this repository.
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    source = ROOT / "apps/lvgl-esp32p4/src/lib.rs"
    manifest = f'''[package]
name = "plico-lvgl-esp32p4"
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
oxivgl = {{ path = {json.dumps(str(oxi))} }}
[profile.release]
opt-level = "s"
panic = "abort"
lto = "fat"
codegen-units = 1
'''
    write(STAGE / "Cargo.toml", manifest)
    env = os.environ.copy()
    env.update(PLICO_LVGL_CLANG_ARGS=str(args_file), PLICO_CC=str(cc),
               CARGO_TARGET_DIR=str(STAGE / "build"), RUSTFLAGS="--cfg espidf_time64")
    # Use the actual rustup toolchain executables, bypassing local cache wrappers.
    cargo = subprocess.check_output(["rustup", "which", "--toolchain", TOOLCHAIN, "cargo"], text=True).strip()
    env["RUSTC"] = subprocess.check_output(["rustup", "which", "--toolchain", TOOLCHAIN, "rustc"], text=True).strip()
    command = [cargo, "build", "--release", "--target", "riscv32imafc-esp-espidf",
               "-Zbuild-std=std,panic_abort"]
    # First resolution creates the isolated lockfile; subsequent builds lock it.
    if (STAGE / "Cargo.lock").exists():
        command.append("--locked")
    run(command, cwd=STAGE, env=env)
    print(STAGE / "build/riscv32imafc-esp-espidf/release/libplico_lvgl_esp32p4.a")


if __name__ == "__main__":
    main()
