//! nav_menu composes Route-derived metadata with Router navigation.
use plico::Router;
use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, plico::Route)]
enum Nav {
    #[default]
    #[route(id = "home", label = "Home")]
    Home,
    #[route(id = "settings", label = "Settings")]
    Settings,
}
struct Menu {
    router: Router<Nav>,
}
impl View for Menu {
    fn show(&mut self, cx: &mut UiContext) {
        nav_menu(cx, &mut self.router);
    }
}

#[test]
fn nav_menu_renders_every_route_and_navigates_on_click() {
    let mut s = Source::new(Menu {
        router: Router::new(Nav::Home),
    });
    let mut tcx = TextContext::new();
    let _ = s.build_frame(&mut tcx, 300, 400);

    let semantics = s.adapter.semantics(&s.engine, 0);
    let home = semantics
        .node_by_role(SemanticRole::Button, "Home", None)
        .unwrap();
    let settings = semantics
        .node_by_role(SemanticRole::Button, "Settings", None)
        .unwrap();
    assert_eq!(home.states.selected, Some(true));
    assert_eq!(settings.states.selected, Some(false));

    assert!(s.handle_semantic_action(SemanticAction::Click {
        target: settings.id
    }));
    let _ = s.build_frame(&mut tcx, 300, 400);
    assert!(s.view.router.show(Nav::Settings));
    let semantics = s.adapter.semantics(&s.engine, 0);
    assert_eq!(
        semantics
            .node_by_role(SemanticRole::Button, "Settings", None)
            .unwrap()
            .states
            .selected,
        Some(true)
    );
    assert_eq!(
        semantics
            .node_by_role(SemanticRole::Button, "Home", None)
            .unwrap()
            .states
            .selected,
        Some(false)
    );
    assert_eq!(s.view.router.back(), Nav::Home);
}
