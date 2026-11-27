use plico::prelude::*;
use plico_renderer::{
    FrameSource, SemanticAction, SemanticRole, TextContext, UiEvent, WindowMetrics,
};
use plico_runtime::testing::{Command, Tester};
use plico_runtime::{HostView, Source};
use std::time::Duration;

struct Controls {
    theme: Theme,
    field: TextBuffer,
    query: String,
    menu: PopoverState,
    select: PopoverState,
    selected: usize,
    checked: bool,
    enabled: bool,
}

impl HostView for Controls {
    fn theme(&self) -> Theme {
        self.theme
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width(280.).align(Align::Start) + |cx| {
            button(cx).text("Button");
            button(cx).text("Primary").primary();
            toolbar(cx) + |cx| {
                button(cx).text("Toolbar button");
            };
            text_input(cx, "Text field", &self.field);
            menu(cx, &mut self.menu, "Actions") + |cx| {
                menu_item(cx, "Refresh");
            };
            let choices = [plico::SelectOption::new("daily", 0, "Daily")];
            select(cx, &mut self.select, "Frequency", &mut self.selected, &choices);
            checkbox(cx, "Keep history", &mut self.checked);
            switch(cx, "Notifications", &mut self.enabled);
            radio(cx, "Local", true);
            button(cx).text("Before search");
            search_field(cx, "Search", "Search", &mut self.query);
            button(cx).text("After search");
        };
    }
}

#[test]
fn default_desktop_controls_align_in_both_densities() {
    for metrics in [Metrics::default(), Metrics::compact()] {
        let mut empty_search = None;
        for query in ["", "invoice"] {
            let mut source = Source::new(Controls {
                theme: Theme {
                    metrics,
                    ..Theme::default()
                },
                field: TextBuffer::new(),
                query: query.into(),
                menu: PopoverState::default(),
                select: PopoverState::default(),
                selected: 0,
                checked: false,
                enabled: true,
            });
            source.build_frame(&mut TextContext::new(), 640, 540);
            let semantics = source.semantics();
            let bounds = |role, label| {
                semantics
                    .node_by_role(role, label, None)
                    .unwrap_or_else(|| panic!("missing {label}"))
                    .bounds
            };
            let height = f32::from(metrics.control_height);
            for (role, label) in [
                (SemanticRole::Button, "Button"),
                (SemanticRole::Button, "Primary"),
                (SemanticRole::Button, "Toolbar button"),
                (SemanticRole::TextInput, "Text field"),
                (SemanticRole::Button, "Actions"),
                (SemanticRole::ComboBox, "Frequency"),
                (SemanticRole::CheckBox, "Keep history"),
                (SemanticRole::Switch, "Notifications"),
                (SemanticRole::RadioButton, "Local"),
            ] {
                let [_, top, _, bottom] = bounds(role, label);
                assert_eq!(bottom - top, height, "{label}");
            }
            let before = bounds(SemanticRole::Button, "Before search")[3];
            let after = bounds(SemanticRole::Button, "After search")[1];
            assert_eq!(after - before, height, "search with {query:?}");
            let [x0, y0, x1, y1] = bounds(SemanticRole::TextInput, "Search");
            assert!(x1 > x0 && y1 > y0 && y0 > before && y1 < after);
            if let Some(bounds) = empty_search.replace([x0, y0, x1, y1]) {
                assert_eq!(
                    bounds,
                    [x0, y0, x1, y1],
                    "the clear button has a reserved slot"
                );
            }
        }
    }
}

#[derive(Default)]
struct RadioDemo {
    selected: bool,
    declarations: usize,
}

impl HostView for RadioDemo {
    fn show(&mut self, cx: &mut UiContext) {
        self.declarations += 1;
        if radio(cx, "Local", self.selected).clicked() {
            self.selected = !self.selected;
            cx.request_rebuild();
        }
    }
}

#[test]
fn radio_uses_retained_motion_and_respects_reduced_motion() {
    for reduced_motion in [false, true] {
        let clock = plico::VirtualClock::default();
        let mut source = Source::with_clock(RadioDemo::default(), clock.clone());
        source.handle_event(UiEvent::WindowMetrics(WindowMetrics {
            reduced_motion,
            ..Default::default()
        }));
        let mut text = TextContext::new();
        source.build_frame(&mut text, 240, 120);
        let mark = |source: &Source<RadioDemo>| {
            let node = source
                .placed_nodes()
                .iter()
                .find(|node| node.rect[2] - node.rect[0] == 6.)
                .expect("the radio mark stays mounted");
            (node.rect, node.paint.opacity)
        };
        let (bounds, off) = mark(&source);
        assert_eq!(off, 0.);
        let target = source
            .semantics()
            .node_by_role(SemanticRole::RadioButton, "Local", None)
            .unwrap()
            .id;
        assert!(source.handle_semantic_action(SemanticAction::Click { target }));
        source.build_frame(&mut text, 240, 120);
        assert!(source.view().selected);
        let declarations = source.view().declarations;
        if reduced_motion {
            assert_eq!(mark(&source), (bounds, 1.));
            assert!(!source.has_anim());
        } else {
            assert_eq!(mark(&source), (bounds, 0.));
            clock.advance(Duration::from_millis(70)).unwrap();
            source.build_frame(&mut text, 240, 120);
            let (rect, opacity) = mark(&source);
            assert_eq!(rect, bounds);
            assert!(opacity > 0. && opacity < 1.);
            clock.advance(Duration::from_millis(70)).unwrap();
            source.build_frame(&mut text, 240, 120);
            assert_eq!(mark(&source), (bounds, 1.));
            assert!(!source.has_anim());
        }
        assert_eq!(source.view().declarations, declarations);
    }
}

#[derive(Default)]
struct Navigation {
    entry: usize,
    tab: usize,
}

impl HostView for Navigation {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        sidebar(cx).width(180.).height(120.) + |cx| {
            let current = self.entry;
            if sidebar_item(cx, "Inbox", current == 0, None).clicked() {
                self.entry = 0;
            }
            if sidebar_item(cx, "Archive", current == 1, None).clicked() {
                self.entry = 1;
            }
        };
        tabs(cx, &mut self.tab, &["General", "Appearance"]);
    }
}

#[test]
fn pressing_another_sidebar_entry_keeps_the_old_selection_neutral() {
    let mut ui = Tester::new(Navigation::default(), 320, 220).unwrap();
    let bounds = |ui: &Tester<Navigation>, label| {
        ui.snapshot()
            .node_by_role(SemanticRole::TreeItem, label, None)
            .unwrap()
            .bounds
    };
    let face = |ui: &Tester<Navigation>, label| {
        ui.placed_nodes()
            .iter()
            .find(|node| node.rect == bounds(ui, label))
            .and_then(|node| node.paint.background)
            .map(|color| color.to_rgba8())
    };
    let neutral = face(&ui, "Inbox").unwrap();
    let [x0, y0, x1, y1] = bounds(&ui, "Archive");
    let (x, y) = (f64::from((x0 + x1) / 2.), f64::from((y0 + y1) / 2.));
    ui.move_pointer(x, y).unwrap();
    ui.press_at(x, y).unwrap();
    ui.advance(Duration::from_millis(150)).unwrap();
    assert_eq!(ui.view().entry, 0);
    assert_eq!(face(&ui, "Inbox"), Some(neutral));
    ui.release_at(x, y).unwrap();
    assert_eq!(ui.view().entry, 1);
    assert_eq!(face(&ui, "Inbox"), None);
    ui.execute(Command::Focus {
        role: "treeitem".into(),
        label: "Archive".into(),
        occurrence: None,
    })
    .unwrap();
    ui.advance(Duration::from_millis(150)).unwrap();
    assert_eq!(ui.view().entry, 1);
    let accent = face(&ui, "Archive").unwrap();
    assert_eq!(
        u32::from(accent.r) << 16 | u32::from(accent.g) << 8 | u32::from(accent.b),
        Theme::light().accent
    );
}

#[test]
fn tab_selection_fades_once_without_resizing_either_label() {
    let mut ui = Tester::new(Navigation::default(), 320, 220).unwrap();
    let bounds = |ui: &Tester<Navigation>, label| {
        ui.snapshot()
            .node_by_role(SemanticRole::Tab, label, None)
            .unwrap()
            .bounds
    };
    let general = bounds(&ui, "General");
    let appearance = bounds(&ui, "Appearance");
    let underline = |ui: &Tester<Navigation>, [x0, y0, x1, y1]: [f32; 4]| {
        ui.placed_nodes()
            .iter()
            .find(|node| {
                let [l, t, r, b] = node.rect;
                l >= x0 && r <= x1 && t >= y0 && b <= y1 && b - t == 2.
            })
            .unwrap()
            .paint
            .opacity
    };
    assert_eq!(underline(&ui, general), 1.);
    assert_eq!(underline(&ui, appearance), 0.);
    ui.click("Appearance").unwrap();
    assert_eq!(ui.view().tab, 1);
    assert_eq!(bounds(&ui, "General"), general);
    assert_eq!(bounds(&ui, "Appearance"), appearance);
    ui.advance(Duration::from_millis(70)).unwrap();
    for opacity in [underline(&ui, general), underline(&ui, appearance)] {
        assert!(opacity > 0. && opacity < 1.);
    }
    ui.advance(Duration::from_millis(70)).unwrap();
    assert_eq!(underline(&ui, general), 0.);
    assert_eq!(underline(&ui, appearance), 1.);
}
