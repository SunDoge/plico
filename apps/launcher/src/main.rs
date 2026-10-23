fn main() -> Result<(), plico_backend::DesktopError> {
    plico_backend::run_view_with_options(
        plico_launcher::Launcher::default(),
        plico_backend::WindowOptions::new()
            .title("Plico launcher")
            .initial_inner_size(680, 526)
            .min_inner_size(520, 526)
            .decorations(false)
            .corner_radius(14)
            .resizable(false)
            .centered(true)
            .window_level(plico_backend::WindowLevel::AlwaysOnTop),
    )
}
