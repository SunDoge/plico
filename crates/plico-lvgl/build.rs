fn main() {
    println!("cargo:rerun-if-changed=src/sdl_close.c");
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    build_thorvg();
    let sdl = pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("sdl2")
        .expect("SDL2 development files required for native LVGL; install libsdl2-dev");
    let mut build = cc::Build::new();
    build.file("src/sdl_close.c");
    for include in sdl.include_paths {
        build.include(include);
    }
    build.compile("plico_sdl_close");
}

// lightvgl-sys 9.5.4 compiles .c sources only, omitting its bundled ThorVG
// .cpp files. Compile those same sources for the native preview, with the same
// lv_conf.h. ESP-IDF builds ThorVG itself and does not enter this path.
fn build_thorvg() {
    use std::path::PathBuf;
    println!("cargo:rerun-if-env-changed=PLICO_LVGL_SOURCE");
    println!("cargo:rerun-if-env-changed=DEP_LV_CONFIG_PATH");
    let config = PathBuf::from(std::env::var_os("DEP_LV_CONFIG_PATH").expect("LVGL config path"));
    println!(
        "cargo:rerun-if-changed={}",
        config.join("lv_conf.h").display()
    );
    let enabled = std::fs::read_to_string(config.join("lv_conf.h"))
        .unwrap()
        .lines()
        .any(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.starts_with(&["#define", "LV_USE_THORVG_INTERNAL", "1"])
        });
    if !enabled {
        return;
    }
    let source = std::env::var_os("PLICO_LVGL_SOURCE")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("CARGO_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo"));
            std::fs::read_dir(home.join("registry/src"))
                .ok()?
                .filter_map(Result::ok)
                .map(|entry| entry.path().join("lightvgl-sys-9.5.4/vendor/lvgl"))
                .find(|path| path.join("src/libs/thorvg/tvgCapi.cpp").exists())
        })
        .expect("Set PLICO_LVGL_SOURCE to lightvgl-sys 9.5.4's vendor/lvgl directory");
    let thorvg = source.join("src/libs/thorvg");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .warnings(false)
        .define("LV_CONF_INCLUDE_SIMPLE", "1")
        .include(config)
        .include(&source);
    for entry in std::fs::read_dir(thorvg).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "cpp") {
            println!("cargo:rerun-if-changed={}", path.display());
            build.file(path);
        }
    }
    build.compile("plico_thorvg");
}
