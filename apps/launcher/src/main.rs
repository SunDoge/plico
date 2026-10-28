fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run_view_with_options(
        launcher::Launcher::default(),
        plico_desktop::WindowOptions::new()
            .title("Plico launcher")
            .initial_inner_size(680, 526)
            .min_inner_size(520, 526)
            .decorations(false)
            .corner_radius(14)
            .resizable(false)
            .centered(true)
            .window_level(plico_desktop::WindowLevel::AlwaysOnTop),
    )
}
