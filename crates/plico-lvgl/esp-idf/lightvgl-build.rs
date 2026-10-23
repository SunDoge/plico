// Bind lightvgl-sys to IDF's exact LVGL headers and sdkconfig. Compile inline
// wrappers only: IDF supplies the sole LVGL library and owns initialization.
use std::{env, fs, path::PathBuf};

fn main() {
    let args_file = env::var("PLICO_LVGL_CLANG_ARGS").expect("Run build-esp32p4.py");
    println!("cargo:rerun-if-env-changed=PLICO_LVGL_CLANG_ARGS");
    println!("cargo:rerun-if-changed={args_file}");
    let args: Vec<_> = fs::read_to_string(&args_file).unwrap().lines().map(str::to_owned).collect();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let header = out.join("plico_lvgl.h");
    fs::write(&header, "#include <lvgl.h>\n").unwrap();
    bindgen::Builder::default()
        .header(header.to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args(&args).clang_arg("-DLV_USE_PRIVATE_API=1")
        .generate_comments(false).derive_default(true).layout_tests(false)
        .use_core().ctypes_prefix("core::ffi").wrap_unsafe_ops(true)
        .wrap_static_fns(true).wrap_static_fns_path(out.join("static_fns.c"))
        .generate().expect("Generate bindings from IDF LVGL configuration")
        .write_to_file(out.join("bindings.rs")).unwrap();
    if out.join("static_fns.c").exists() {
        let mut cc = cc::Build::new();
        cc.compiler(env::var("PLICO_CC").unwrap());
        for arg in &args {
            if !arg.starts_with("--target=") { cc.flag(arg); }
        }
        cc.flag("-DLV_USE_PRIVATE_API=1").include(&out)
            .file(out.join("static_fns.c")).warnings(false).compile("plico_lvgl_inline");
    }
}
