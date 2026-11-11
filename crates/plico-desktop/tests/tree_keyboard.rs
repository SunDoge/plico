use plico::VirtualClock;
use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};

#[derive(Default)]
struct TreeDemo {
    open: bool,
}

impl HostView for TreeDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Before");
        tree(cx, |cx| {
            tree_item(cx, "src", true, Some(&mut self.open), 0, |cx| {
                tree_item(cx, "main.rs", false, None, 1, |_| {});
                tree_item(cx, "lib.rs", false, None, 1, |_| {});
            });
            tree_item(cx, "Cargo.toml", false, None, 0, |_| {});
        });
        button(cx).text("After");
    }
}

fn key(name: &str) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: name.into(),
        code: name.into(),
        key_without_modifiers: name.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        synthetic: false,
    })
}

fn id(source: &Source<TreeDemo>, role: SemanticRole, label: &str) -> u64 {
    source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(role, label, None)
        .unwrap()
        .id
}

fn focus(source: &Source<TreeDemo>) -> Option<u64> {
    source.adapter.semantics(&source.engine, 0).focus
}

#[test]
fn tree_is_one_tab_stop_and_arrows_follow_visible_parent_child_order() {
    let mut source = Source::new(TreeDemo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 300);

    source.handle_event(key("Tab"));
    assert_eq!(
        focus(&source),
        Some(id(&source, SemanticRole::Button, "Before"))
    );
    source.handle_event(key("Tab"));
    assert_eq!(
        focus(&source),
        Some(id(&source, SemanticRole::TreeItem, "src"))
    );
    source.handle_event(key("Tab"));
    assert_eq!(
        focus(&source),
        Some(id(&source, SemanticRole::Button, "After"))
    );

    let src = id(&source, SemanticRole::TreeItem, "src");
    assert!(source.handle_semantic_action(SemanticAction::Focus { target: src }));
    source.handle_event(key("ArrowRight"));
    source.build_frame(&mut text, 400, 300);
    assert!(source.view.open);
    assert_eq!(focus(&source), Some(src));

    source.handle_event(key("ArrowRight"));
    assert_eq!(
        focus(&source),
        Some(id(&source, SemanticRole::TreeItem, "main.rs"))
    );
    source.handle_event(key("ArrowLeft"));
    assert_eq!(focus(&source), Some(src));
    source.handle_event(key("ArrowLeft"));
    source.build_frame(&mut text, 400, 300);
    assert!(!source.view.open);
    assert_eq!(focus(&source), Some(src));
}

#[test]
fn tree_navigation_clamps_and_first_letter_search_wraps() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(TreeDemo { open: true }, clock.clone());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 300);
    let src = id(&source, SemanticRole::TreeItem, "src");
    assert!(source.handle_semantic_action(SemanticAction::Focus { target: src }));

    source.handle_event(key("ArrowUp"));
    assert_eq!(focus(&source), Some(src));
    source.handle_event(key("End"));
    let cargo = id(&source, SemanticRole::TreeItem, "Cargo.toml");
    assert_eq!(focus(&source), Some(cargo));
    source.handle_event(key("ArrowDown"));
    assert_eq!(focus(&source), Some(cargo));
    source.handle_event(key("m"));
    assert_eq!(
        focus(&source),
        Some(id(&source, SemanticRole::TreeItem, "main.rs"))
    );
    clock.advance(std::time::Duration::from_secs(2)).unwrap();
    source.handle_event(key("c"));
    assert_eq!(focus(&source), Some(cargo));
    clock.advance(std::time::Duration::from_secs(2)).unwrap();
    source.handle_event(key("l"));
    let lib = id(&source, SemanticRole::TreeItem, "lib.rs");
    assert_eq!(focus(&source), Some(lib));
    source.handle_event(key("i"));
    assert_eq!(focus(&source), Some(lib));
}

#[derive(Default)]
struct AccordionDemo {
    general: bool,
    privacy: bool,
}

impl HostView for AccordionDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        accordion(cx, |cx| {
            collapsible(cx, "General", &mut self.general, |_| {});
            collapsible(cx, "Privacy", &mut self.privacy, |_| {});
        });
    }
}

#[test]
fn accordion_headers_follow_mygo_vertical_focus_group_keys() {
    let mut source = Source::new(AccordionDemo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 300);
    let general = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "General", None)
        .unwrap()
        .id;
    let privacy = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Privacy", None)
        .unwrap()
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Focus { target: general }));
    source.handle_event(key("ArrowUp"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(general)
    );
    source.handle_event(key("ArrowDown"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(privacy)
    );
    source.handle_event(key("ArrowDown"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(privacy)
    );
    source.handle_event(key("Home"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(general)
    );
    source.handle_event(key("End"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(privacy)
    );
}

struct SidebarDemo {
    selected: u8,
    open: bool,
}

impl Default for SidebarDemo {
    fn default() -> Self {
        Self {
            selected: 0,
            open: true,
        }
    }
}

impl HostView for SidebarDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Before");
        sidebar(cx).width(220.).height(180.).children(|cx| {
            sidebar_section(cx, "Mailboxes", Some(&mut self.open), |cx| {
                if sidebar_item(cx, "Inbox", self.selected == 0, None).clicked() {
                    self.selected = 0;
                }
                if sidebar_item(cx, "Archive", self.selected == 1, None).clicked() {
                    self.selected = 1;
                }
            });
        });
        button(cx).text("After");
    }
}

#[test]
fn sidebar_has_one_selected_tab_stop_and_navigation_moves_selection() {
    let mut source = Source::new(SidebarDemo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 300);
    let semantic = |source: &Source<SidebarDemo>, role, label| {
        source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(role, label, None)
            .unwrap()
            .id
    };

    source.handle_event(key("Tab"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(semantic(&source, SemanticRole::Button, "Before"))
    );
    source.handle_event(key("Tab"));
    let inbox = semantic(&source, SemanticRole::TreeItem, "Inbox");
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(inbox)
    );
    source.handle_event(key("Tab"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(semantic(&source, SemanticRole::Button, "After"))
    );

    assert!(source.handle_semantic_action(SemanticAction::Focus { target: inbox }));
    source.handle_event(key("ArrowDown"));
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.selected, 1);
    let archive = semantic(&source, SemanticRole::TreeItem, "Archive");
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(archive)
    );
    source.handle_event(key("Home"));
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.selected, 0);
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(inbox)
    );
}

struct RadioDemo {
    selected: u8,
}

impl Default for RadioDemo {
    fn default() -> Self {
        Self { selected: 1 }
    }
}

impl HostView for RadioDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Before");
        radio_group(cx, |cx| {
            if radio(cx, "Small", self.selected == 0).clicked() {
                self.selected = 0;
            }
            if radio(cx, "Medium", self.selected == 1).clicked() {
                self.selected = 1;
            }
            if radio(cx, "Large", self.selected == 2).clicked() {
                self.selected = 2;
            }
        });
        button(cx).text("After");
    }
}

fn radio_id(source: &Source<RadioDemo>, label: &str) -> u64 {
    let semantics = source.adapter.semantics(&source.engine, 0);
    semantics
        .node_by_role(SemanticRole::RadioButton, label, None)
        .unwrap_or_else(|| panic!("missing radio {label:?}: {:?}", semantics.nodes))
        .id
}

#[test]
fn radio_group_is_one_tab_stop_and_direction_keys_choose() {
    let mut source = Source::new(RadioDemo::default());
    let mut text = TextContext::new();
    let placed = source.build_frame(&mut text, 720, 400);
    assert!(!placed.is_empty(), "{:?}", source.last_error());
    assert!(!source.adapter.semantics(&source.engine, 0).nodes.is_empty());

    source.handle_event(key("Tab"));
    source.handle_event(key("Tab"));
    let medium = radio_id(&source, "Medium");
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(medium)
    );
    source.handle_event(key("Tab"));
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(
            source
                .adapter
                .semantics(&source.engine, 0)
                .node_by_role(SemanticRole::Button, "After", None)
                .unwrap()
                .id
        )
    );

    assert!(source.handle_semantic_action(SemanticAction::Focus { target: medium }));
    source.handle_event(key("ArrowDown"));
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.selected, 2);
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(radio_id(&source, "Large"))
    );

    source.handle_event(key("ArrowRight"));
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.selected, 0);
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(radio_id(&source, "Small"))
    );

    source.handle_event(key("End"));
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.selected, 2);
    source.handle_event(key("Home"));
    source.build_frame(&mut text, 400, 300);
    assert_eq!(source.view.selected, 0);
}

#[derive(Default)]
struct RatingDemo {
    stars: u8,
}

impl HostView for RatingDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Before");
        rating_labeled(cx, "Stars", &mut self.stars, 5);
        button(cx).text("After");
    }
}

#[test]
fn rating_is_one_slider_tab_stop_with_range_keys() {
    let mut source = Source::new(RatingDemo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 500, 240);

    source.handle_event(key("Tab"));
    source.handle_event(key("Tab"));
    let slider = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Slider, "Stars", None)
        .unwrap()
        .id;
    assert!(
        source
            .adapter
            .semantics(&source.engine, 0)
            .node_by_role(SemanticRole::Button, "1 of 5 stars", None)
            .is_none(),
        "the visual stars must not duplicate the slider semantics"
    );
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(slider)
    );

    source.handle_event(key("ArrowRight"));
    source.build_frame(&mut text, 500, 240);
    source.handle_event(key("ArrowUp"));
    source.build_frame(&mut text, 500, 240);
    assert_eq!(source.view.stars, 2);

    source.handle_event(key("End"));
    source.build_frame(&mut text, 500, 240);
    assert_eq!(source.view.stars, 5);
    source.handle_event(key("Home"));
    source.build_frame(&mut text, 500, 240);
    assert_eq!(source.view.stars, 0);

    source.handle_event(key("Tab"));
    let after = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "After", None)
        .unwrap()
        .id;
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(after)
    );
}

struct StepperDemo {
    value: f64,
}

impl Default for StepperDemo {
    fn default() -> Self {
        Self { value: 2. }
    }
}

impl HostView for StepperDemo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Before");
        stepper_labeled(cx, "Copies", &mut self.value, 0. ..=5., 0.5);
        button(cx).text("After");
    }
}

#[test]
fn stepper_is_one_spin_button_tab_stop_with_range_keys() {
    let mut source = Source::new(StepperDemo::default());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 240);
    source.handle_event(key("Tab"));
    source.handle_event(key("Tab"));
    let semantics = source.adapter.semantics(&source.engine, 0);
    let stepper = semantics
        .node_by_role(SemanticRole::SpinButton, "Copies", None)
        .unwrap()
        .id;
    assert_eq!(semantics.focus, Some(stepper));
    assert!(
        semantics
            .node_by_role(SemanticRole::SpinButton, "Increase", None)
            .is_none()
    );

    source.handle_event(key("ArrowUp"));
    source.build_frame(&mut text, 400, 240);
    assert_eq!(source.view.value, 2.5);
    source.handle_event(key("ArrowLeft"));
    source.build_frame(&mut text, 400, 240);
    assert_eq!(source.view.value, 2.);
    source.handle_event(key("End"));
    source.build_frame(&mut text, 400, 240);
    assert_eq!(source.view.value, 5.);
    source.handle_event(key("Home"));
    source.build_frame(&mut text, 400, 240);
    assert_eq!(source.view.value, 0.);

    source.handle_event(key("Tab"));
    let after = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "After", None)
        .unwrap()
        .id;
    assert_eq!(
        source.adapter.semantics(&source.engine, 0).focus,
        Some(after)
    );
}
