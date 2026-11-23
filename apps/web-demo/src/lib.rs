//! Small UI-only module: deliberately no renderer, platform shell or host resources.
pub mod icons;
use plico::prelude::*;
use plico_dev::services::ServiceApp;
use serde::{Deserialize, Serialize};

pub mod protocol;
use protocol::ServiceSnapshot;
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

    fn on_key_event(
        &mut self,
        key: &plico::KeyEvent,
        _: &mut plico_dev::services::Services<ServiceSnapshot, ()>,
    ) -> bool {
        if plico::Shortcut::physical("ArrowUp", plico::Modifiers::ALT).matches(key) {
            self.count += 1;
            return true;
        }
        false
    }

    fn show(
        &mut self,
        cx: &mut UiContext,
        services: &mut plico_dev::services::Services<ServiceSnapshot, ()>,
    ) {
        let _ = column(cx)
            .height_percent(100.)
            .scroll_y(true)
            .padding(32)
            .gap(16)
            .children(|cx| {
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
                text(cx, "Click Increment or press Alt + ↑.").key("subtitle");
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
#[cfg(all(feature = "guest", target_arch = "wasm32"))]
plico_wasm::export_ui!(
    protocol::ABI,
    protocol::CAPACITY,
    protocol::Input,
    |input: &protocol::ArchivedInput, output| {
        let Ok(input) = rkyv::deserialize::<_, rkyv::rancor::Error>(input) else {
            return 0;
        };
        let base = input.base;
        let key_probe = !input.key_events.is_empty();
        let Ok(mut frame) = protocol::render(input) else {
            return 0;
        };
        use plico_dev::tree_update::{TextPatch, TreeUpdate};
        thread_local! {
            static LAST: std::cell::RefCell<(u64, Vec<plico::ViewNode>)> = const { std::cell::RefCell::new((0, Vec::new())) };
        }
        LAST.with_borrow_mut(|(revision, previous)| {
            frame.revision = *revision + 1;
            let mut nodes = std::mem::take(&mut frame.nodes);
            let update = base
                .filter(|r| *r == *revision && *r != 0)
                .and_then(|_| TextPatch::between(previous, &nodes).map(TreeUpdate::Text));
            let reply = frame.map_nodes(|_| {
                update.unwrap_or_else(|| TreeUpdate::Full(std::mem::take(&mut nodes)))
            });
            let Ok(len) = plico_wasm::write_archive(&reply, output) else {
                return 0;
            };
            // Only accepted shortcut frames become the host's next baseline.
            if key_probe && !reply.key_handled {
                return len as u32;
            }
            *revision = reply.revision;
            *previous = match reply.nodes {
                TreeUpdate::Full(full) => full,
                TreeUpdate::Text(_) => nodes,
            };
            len as u32
        })
    }
);

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
        let frame = protocol::render(protocol::Input {
            services: ServiceSnapshot {
                instance: 7,
                progress: 42,
            },
            ..Default::default()
        })
        .unwrap();

        assert!(has_text(&frame.nodes, "Mock host service #7 · progress 42"));
        assert_eq!(
            frame.state.decode::<serde_json::Value>().unwrap(),
            serde_json::json!({"count": 0})
        );
    }
}
