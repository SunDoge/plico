//! Desktop paradigms demo: tab strips, toolbars, toasts and drag surfaces
//! (the shared widgets in plico::widgets, MyGO semantics).
use crate::heading;
use plico::prelude::*;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Patterns {
    tab: usize,
    toasts: ToastHost,
    toast_count: u32,
    undoable: u32,
    dot: (f64, f64),
}

impl Patterns {
    pub fn tick(&mut self) -> bool {
        !self.toasts.is_empty()
    }
}

#[component]
#[rustfmt::skip]
pub fn page(ui: &mut UiContext, state: &mut Patterns) {
    heading(ui, "Desktop patterns", "Tabs, toolbars, toasts and drag surfaces — the shared desktop widgets.");
    toolbar(ui) + |ui| {
        if button(ui).text("Toast").primary().clicked() {
            state.toast_count += 1;
            state.toasts.toast(Toast::message(format!("Saved {} times", state.toast_count)));
        }
        if button(ui).text("Undoable action").ghost().clicked() {
            state.undoable += 1;
            let undone = state.undoable;
            state.toasts.toast(Toast::message(format!("Deleted item {undone}")).with_action("Undo", || {}));
        }
        if button(ui).text("Failure").ghost().clicked() {
            state.toasts.toast(Toast {
                kind: ToastKind::Error,
                description: Some("The disk filled up mid-write.".into()),
                ..Toast::message("Save failed")
            });
        }
        if button(ui).text("Sticky").ghost().clicked() {
            state.toasts.toast(Toast::message("Still here until closed").sticky());
        }
        toolbar_spacer(ui);
        text!(ui, "{} in flight", state.toasts.len()).font_size(12).color(Color::Muted);
    };
    separator(ui);
    if tabs(ui, &mut state.tab, &["Overview", "Details", "Activity"]) {
        ui.request_rebuild();
    }
    column(ui).padding(12).gap(8).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
        match state.tab {
            0 => {
                text(ui, "Overview").font_size(16).weight(650);
                text(ui, "Click a tab above; the strip reports the change and the tab underlines with the accent color.")
                    .color(Color::Muted);
            }
            1 => {
                text(ui, "Details").font_size(16).weight(650);
                text(ui, "Each tab keeps its semantic role for screen readers; the strip is one focused group.")
                    .color(Color::Muted);
            }
            _ => {
                text(ui, "Activity").font_size(16).weight(650);
                text(ui, "Keyboard traversal follows MyGO's Tabs; adapter focus wiring is the remaining step.")
                    .color(Color::Muted);
            }
        };
    };
    // The viewport shows the toolbar's toasts; declared after the content
    // so toasts layer over it (a real app declares this once at the root).
    let showing = toast_viewport(ui, &mut state.toasts);
    let _ = showing;
    column(ui).padding(12).gap(8).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
        text(ui, "Drag surface").font_size(16).weight(650);
        text(ui, "Hold the pointer down in the panel and move it: the dot follows the drag.").color(Color::Muted);
        row(ui).height(120.) + |ui| {
            let (x, y) = state.dot;
            column(ui).grow(1.).radius(8.).background(Color::Surface).border(1., Color::Border) + |ui| {
                drag_area(ui, &mut |dx, dy| {
                    state.dot.0 = (state.dot.0 + dx).clamp(-80., 80.);
                    state.dot.1 = (state.dot.1 + dy).clamp(-40., 40.);
                });
                row(ui).align(Align::Center).justify(Justify::Center).grow(1.) + |ui| {
                    column(ui).width(14.).height(14.).radius(7.).background(Color::Accent)
                        .translate(x as f32, y as f32).children(|_| {});
                };
            };
        };
    };
}
