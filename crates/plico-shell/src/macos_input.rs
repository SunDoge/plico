//! Native click threshold; MyGO uses NSEvent click counts directly, while
//! winit exposes button events without that native count. See third_party.
use plico_renderer::MultiClickSettings;

pub(crate) fn multi_click_settings() -> MultiClickSettings {
    let mut settings = MultiClickSettings::default();
    if let Ok(interval) =
        std::time::Duration::try_from_secs_f64(objc2_app_kit::NSEvent::doubleClickInterval())
        && !interval.is_zero()
    {
        settings.interval = interval;
    }
    settings
}
