//! Right-click near the window edge to open a desktop context menu.
use peniko::Color;
use plico_renderer::{
    FrameSource, TextContext, WindowOptions,
    layout::PlacedNode,
    source::{EventResponse, PointerButton, PointerPhase, UiEvent},
};
use plico_shell::{
    ContextMenuItem, ContextMenuRequest, EffectCompletion, EffectId, EffectPayload, EffectRequest,
    EffectScope, initial_window_resource_key,
};

#[derive(Default)]
struct Demo {
    pending: Option<EffectRequest>,
    sequence: u32,
}
fn item(id: &str, label: &str, enabled: bool, checked: bool) -> ContextMenuItem {
    ContextMenuItem::Item {
        id: id.into(),
        label: label.into(),
        enabled,
        checked,
    }
}
impl FrameSource for Demo {
    fn build_frame(&mut self, _: &mut TextContext, _: u32, _: u32) -> Vec<PlacedNode> {
        Vec::new()
    }
    fn base_color(&self) -> Color {
        Color::from_rgb8(220, 221, 224)
    }
    fn handle_event(&mut self, event: UiEvent) -> EventResponse {
        if let UiEvent::Pointer(p) = event
            && p.phase == PointerPhase::Up
            && p.button == Some(PointerButton::Secondary)
        {
            let owner = initial_window_resource_key(0);
            self.sequence += 1;
            self.pending = Some(EffectRequest {
                id: EffectId(self.sequence),
                scope: EffectScope::Window(owner),
                payload: EffectPayload::ContextMenuShow(ContextMenuRequest {
                    window_id: owner,
                    position: None,
                    items: vec![
                        item("copy", "Copy", true, false),
                        item("paste", "Paste", false, false),
                        ContextMenuItem::Separator,
                        ContextMenuItem::Submenu {
                            label: "View".into(),
                            items: vec![item("details", "Show details", true, true)],
                        },
                    ],
                }),
            });
            return EventResponse {
                handled: true,
                request_redraw: true,
                ..Default::default()
            };
        }
        EventResponse::IGNORED
    }
    fn take_effect(&mut self) -> Option<EffectRequest> {
        self.pending.take()
    }
    fn complete_effect(&mut self, completion: EffectCompletion) {
        println!("{:?}", completion.result);
    }
}
fn main() -> plico_shell::Result<()> {
    plico_shell::run_window_with_options(
        Box::new(Demo::default()),
        WindowOptions::new()
            .title("Plico · Right-click for menu")
            .initial_inner_size(360, 200),
    )
}
