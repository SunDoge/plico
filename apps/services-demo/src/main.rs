mod service;
fn main() -> Result<(), plico_vello::ServiceDesktopError> {
    plico_vello::run_service_app_with_options(
        services_demo::Application::default(),
        service::TaskService::new(),
        plico_vello::WindowOptions::new()
            .title("Plico · Task workspace")
            .initial_inner_size(620, 520)
            .min_inner_size(520, 480),
    )
}
