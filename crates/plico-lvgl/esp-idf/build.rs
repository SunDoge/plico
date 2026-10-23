// Adapter for pinned oxivgl-sys 0.2.6: generate from IDF's exact headers/config,
// compile only bindgen's inline wrappers, and link IDF's sole LVGL instance.
use std::{env, fs, path::PathBuf};

fn main() {
    let args_file = env::var("PLICO_LVGL_CLANG_ARGS").expect("Run scripts/build-esp32p4.py");
    println!("cargo:rerun-if-env-changed=PLICO_LVGL_CLANG_ARGS");
    println!("cargo:rerun-if-changed={args_file}");
    let args = fs::read_to_string(&args_file).unwrap().lines().map(str::to_owned).collect::<Vec<_>>();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let shims = root.join("shims");
    let bindings = bindgen::Builder::default()
        .header(shims.join("lvgl_sys.h").to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args(&args)
        .generate_comments(false).derive_default(true).layout_tests(false)
        .use_core().ctypes_prefix("core::ffi").wrap_unsafe_ops(true)
        .wrap_static_fns(true).wrap_static_fns_path(out.join("static_fns.c"))
        .generate().expect("Generate bindings from IDF LVGL configuration");
    let bindings_file = out.join("bindings.rs");
    bindings.write_to_file(&bindings_file).unwrap();
    let src = fs::read_to_string(&bindings_file).unwrap();
    for size in [8,10,12,14,16,18,20,22,24,26,28,30,32,34,36,38,40,42,44,46,48] {
        let name = format!("montserrat_{size}");
        if src.contains(&format!("lv_font_{name} :")) || src.contains(&format!("lv_font_{name}:")) {
            println!("cargo:font_{name}=1");
        }
    }
    let mut cc = cc::Build::new();
    cc.compiler(env::var("PLICO_CC").unwrap());
    for arg in &args {
        if !arg.starts_with("--target=") { cc.flag(arg); }
    }
    cc.include(&shims).file(shims.join("lvgl_sys.c"));
    if out.join("static_fns.c").exists() { cc.file(out.join("static_fns.c")); }
    cc.warnings(false).compile("plico_lvgl_inline");
}
