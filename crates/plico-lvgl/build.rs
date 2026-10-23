fn main() {
    println!("cargo:rerun-if-changed=src/sdl_close.c");
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
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
