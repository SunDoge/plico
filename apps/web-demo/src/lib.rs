//! Small UI-only module: deliberately no renderer, platform shell or host resources.
pub mod icons;
use plico_core::prelude::*;
use plico_dev::{App, Request};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
struct ServiceSnapshot {
    instance: u64,
    progress: u64,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Counter {
    pub count: u64,
    #[serde(skip)]
    service: ServiceSnapshot,
    // Required only in the reset fixture, so an old checkpoint cannot decode.
    #[cfg(feature = "schema-reset")]
    required: bool,
}
impl App for Counter {
    fn set_services(&mut self, value: serde_json::Value) -> plico_dev::Result<()> {
        self.service =
            serde_json::from_value(value).map_err(|source| plico_dev::Error::Json { source })?;
        Ok(())
    }
    fn show(&mut self, cx: &mut UiContext) {
        let _ = column(cx).padding(32).gap(16).children(|cx| {
            text(
                cx,
                if cfg!(feature = "revised") {
                    "Replaceable UI · revision 2"
                } else {
                    "Replaceable UI · revision 1"
                },
            )
            .key("title")
            .font_size(28);
            text(
                cx,
                "The canvas, renderer, font caches and service remain in the host.",
            )
            .key("subtitle");
            svg(cx, &crate::icons::ZAP).key("icon").size(24., 24.);
            text!(cx, "Counter: {}", self.count)
                .key("count")
                .font_size(24);
            row(cx).key("actions").gap(8).children(|cx| {
                if button(cx).text("Increment").key("increment").clicked() {
                    self.count += 1;
                }
                if button(cx).text("Reset counter").key("reset").clicked() {
                    self.count = 0;
                }
            });
            text!(
                cx,
                "Mock host service #{} · progress {}",
                self.service.instance,
                self.service.progress
            )
            .key("service");
        });
    }
}
/// Same Request/Frame JSON as native hot reload; wasm-bindgen handles string transfer.
pub fn render_json(input: &str) -> Result<String, String> {
    let request: Request = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let frame = plico_dev::render::<Counter>(request).map_err(|e| e.to_string())?;
    serde_json::to_string(&frame).map_err(|e| e.to_string())
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn render(input: &str) -> Result<String, wasm_bindgen::JsValue> {
    render_json(input).map_err(|e| wasm_bindgen::JsValue::from_str(&e))
}
