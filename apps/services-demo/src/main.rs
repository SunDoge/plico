mod service;
fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run_source(
        plico_desktop::Source::new(plico_desktop::ServiceView::new(
            services_demo::Application::default(),
            service::TaskService::new(),
        )),
        plico_desktop::WindowOptions::new()
            .title("Plico · Task workspace")
            .initial_inner_size(620, 520)
            .min_inner_size(520, 480),
    )
}
