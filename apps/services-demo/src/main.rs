mod service;
fn main() -> Result<(), plico_desktop::ServiceDesktopError> {
    plico_desktop::run_service_app_with_options(
        services_demo::Application::default(),
        service::TaskService::new(),
        plico_desktop::WindowOptions::new()
            .title("Plico · Task workspace")
            .initial_inner_size(620, 520)
            .min_inner_size(520, 480),
    )
}
