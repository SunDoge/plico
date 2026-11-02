use backend_settings::preferences_regions::{ImmediatePreferences, RegionPreferences};
use plico_renderer::{RenderBackend, WindowOptions};
use plico_runtime::{HostView, Source};
#[derive(usage_rs::Cli)]
#[usage(about = "Same Preferences page through immediate or region API")]
struct Options {
    /// Run the original synchronous page instead of regions.
    #[usage(long)]
    immediate: bool,
}
fn main() -> Result<(), plico_shell::Error> {
    let options = Options::parse();
    if options.immediate {
        run(ImmediatePreferences::default(), "Immediate")
    } else {
        run(RegionPreferences::default(), "Regions")
    }
}
fn run(view: impl HostView + 'static, model: &str) -> Result<(), plico_shell::Error> {
    plico_shell::run_window_with_options(
        Box::new(Source::new(view)),
        WindowOptions::new()
            .title(format!("Preferences · {model}"))
            .initial_inner_size(900, 780)
            .min_inner_size(480, 600)
            .renderer(RenderBackend::Skia),
    )
}
