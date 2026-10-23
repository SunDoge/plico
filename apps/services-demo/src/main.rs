mod service;
fn main() -> Result<(), plico_backend::ServiceDesktopError> {
    plico_backend::run_service_app_with_options(
        plico_services_demo::Application::default(),
        service::TaskService::new(),
        plico_backend::WindowOptions::new()
            .title("Plico · Task workspace")
            .initial_inner_size(620, 520)
            .min_inner_size(520, 480),
    )
}
