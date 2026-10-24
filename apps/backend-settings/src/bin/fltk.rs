fn main() -> plico_fltk::Result<()> {
    plico_fltk::run(
        plico_backend_settings::Settings::default(),
        "Plico settings — FLTK",
        1100,
        820,
    )
}
