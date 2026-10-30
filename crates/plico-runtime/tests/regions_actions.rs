use plico::prelude::*;
use plico_renderer::TextContext;
use plico_runtime::{
    Source,
    regions::{FocusOnce, RegionUi, Regions, RevealOnce},
};

fn frame(source: &mut Source<Regions>, text: &mut TextContext) {
    source.experiment_prepare_without_scene(text, 700, 600);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn find(source: &Source<Regions>, key: &'static str) -> plico::Entity {
    fn walk(
        engine: &plico::Engine,
        parent: plico::Entity,
        key: &'static str,
    ) -> Option<plico::Entity> {
        if let Some(found) = engine.find(parent, key) {
            return Some(found);
        }
        engine
            .node(parent)
            .unwrap()
            .children
            .iter()
            .find_map(|child| walk(engine, *child, key))
    }
    walk(&source.engine, source.engine.root(), key).unwrap()
}
fn clear_focus(focus: plico_runtime::regions::Access<'_, bool>) {
    focus.update_if(|requested| {
        *requested = false;
        false
    });
}
fn clear_reveal(reveal: plico_runtime::regions::Access<'_, Option<usize>>) {
    reveal.update_if(|index| {
        *index = None;
        false
    });
}
#[test]
fn focus_once_survives_unmount_and_matches_full_declaration() {
    let mut handles = None;
    let app = Regions::new(|app| {
        let mounted = app.state(|| true);
        let focus = app.state(|| false);
        handles = Some((mounted, focus));
        app.root((mounted, focus), |cx, (mounted, focus)| {
            cx.region(
                (mounted.handle(), focus.handle()),
                |cx, (mounted, focus)| {
                    if *mounted.read() {
                        text_input(cx, "Search", &TextBuffer::new())
                            .key("search")
                            .request_focus_once(*focus.read(), focus.handle(), clear_focus);
                    }
                },
            );
        });
    });
    let (mounted, focus) = handles.unwrap();
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    let input = find(&source, "search");

    source.view.access(focus).set(true);
    source.try_declare().unwrap();
    assert_eq!(source.engine.take_focus_request(), Some(input));
    assert!(!*source.view.access(focus).read());
    source.try_declare().unwrap();
    assert_eq!(source.engine.take_focus_request(), None);

    source.view.access(focus).set(true);
    source.view.access(mounted).set(false);
    source.try_declare().unwrap();
    assert_eq!(source.engine.take_focus_request(), None);
    assert!(*source.view.access(focus).read());
    source.view.access(mounted).set(true);
    source.try_declare().unwrap();
    let remounted = find(&source, "search");
    assert_eq!(source.engine.take_focus_request(), Some(remounted));
    assert!(!*source.view.access(focus).read());

    source.view.access(focus).set(true);
    source.engine.set_viewport(701., 600.).unwrap();
    source.try_declare().unwrap();
    assert_eq!(source.engine.take_focus_request(), Some(remounted));
    assert!(!*source.view.access(focus).read());
    source.try_declare().unwrap();
    assert_eq!(source.engine.take_focus_request(), None);
}
#[test]
fn reveal_once_is_not_replayed_by_settling_or_full_declaration() {
    let mut reveal_handle = None;
    let app = Regions::new(|app| {
        let reveal = app.state(|| None::<usize>);
        reveal_handle = Some(reveal);
        app.root((reveal,), |cx, (reveal,)| {
            cx.region((reveal.handle(),), |cx, (reveal,)| {
                virtual_list(cx, 20, 56.)
                    .key("list")
                    .height(56.)
                    .reveal_once(*reveal.read(), reveal.handle(), clear_reveal)
                    .rows(
                        |index| index,
                        |cx, index| {
                            text!(cx, "Row {index}");
                        },
                    );
            });
        });
    });
    let reveal = reveal_handle.unwrap();
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    let list = find(&source, "list");

    source.view.access(reveal).set(Some(10));
    source.try_declare().unwrap();
    assert_eq!(source.engine.take_scroll_requests().get(&list), Some(&560.));
    assert_eq!(*source.view.access(reveal).read(), None);
    source.try_declare().unwrap();
    assert!(source.engine.take_scroll_requests().is_empty());

    source.view.access(reveal).set(Some(4));
    source.engine.set_viewport(701., 600.).unwrap();
    source.try_declare().unwrap();
    assert_eq!(source.engine.take_scroll_requests().get(&list), Some(&224.));
    assert_eq!(*source.view.access(reveal).read(), None);
    source.try_declare().unwrap();
    assert!(source.engine.take_scroll_requests().is_empty());
}
