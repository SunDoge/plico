//! Small UI-only module: deliberately no renderer, platform shell or host resources.
pub mod icons;
use plico::prelude::*;
use plico_dev::{Request, services::ServiceApp};
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct ServiceSnapshot {
    instance: u64,
    progress: u64,
}
#[derive(Default, Serialize, Deserialize)]
struct Counter {
    count: u64,
    // Required only in the reset fixture, so an old checkpoint cannot decode.
    #[cfg(feature = "schema-reset")]
    required: bool,
}
impl ServiceApp for Counter {
    type Snapshot = ServiceSnapshot;
    type Command = ();

    fn show(
        &mut self,
        cx: &mut UiContext,
        services: &mut plico_dev::services::Services<ServiceSnapshot, ()>,
    ) {
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
                services.snapshot().instance,
                services.snapshot().progress
            )
            .key("service");
        });
    }
}
/// Request/Frame JSON entry for the independently retained WASM split host.
pub fn render_json(input: &str) -> Result<String, String> {
    let request: Request = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let frame = plico_dev::services::render::<Counter>(request).map_err(|e| e.to_string())?;
    serde_json::to_string(&frame).map_err(|e| e.to_string())
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn render(input: &str) -> Result<String, wasm_bindgen::JsValue> {
    render_json(input).map_err(|e| wasm_bindgen::JsValue::from_str(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_text(nodes: &[plico::ViewNode], expected: &str) -> bool {
        nodes
            .iter()
            .any(|node| node.text == expected || has_text(&node.children, expected))
    }

    #[test]
    fn typed_services_stay_out_of_the_application_checkpoint() {
        let input = serde_json::to_string(&Request {
            services: serde_json::json!({"instance": 7, "progress": 42}),
            ..Default::default()
        })
        .unwrap();
        let frame: plico_dev::Frame = serde_json::from_str(&render_json(&input).unwrap()).unwrap();

        assert!(has_text(&frame.nodes, "Mock host service #7 · progress 42"));
        assert_eq!(
            frame.state.decode::<serde_json::Value>().unwrap(),
            serde_json::json!({"count": 0})
        );
    }
}
