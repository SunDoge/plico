use plico::{Key, Router, prelude::*};
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_runtime::{
    Source,
    regions::{
        Access, BindState, OnClick, Reactive, Regions, State, RegionUi, outlet, outlet_fill,
        outlet_keyed,
    },
};
use std::{cell::RefCell, rc::Rc};

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, plico::Route,
)]
enum Page {
    #[default]
    Home,
    Editor,
}
#[derive(Clone, Copy)]
struct Local {
    page: Page,
    count: State<i32>,
}
#[derive(Default)]
struct Trace {
    builds: Vec<Page>,
    locals: Vec<Local>,
}
type Records = Rc<RefCell<Trace>>;
struct Harness {
    source: Source<Regions>,
    text: TextContext,
    router: State<Router<Page>>,
    shared: State<i32>,
    trace: Records,
}
impl Harness {
    fn new() -> Self {
        let records = Records::default();
        let (view, router, shared) = program(Rc::clone(&records));
        let mut app = Self {
            source: Source::new(view),
            text: TextContext::new(),
            router,
            shared,
            trace: records,
        };
        app.frame();
        app
    }
    fn frame(&mut self) {
        self.source
            .experiment_prepare_without_scene(&mut self.text, 700, 700);
        assert!(
            self.source.last_error.is_none(),
            "{:?}",
            self.source.last_error
        );
    }
    fn navigate(&mut self, page: Page) {
        self.source.view.access(self.router).update(|r| r.go(page));
        self.frame();
    }
    fn local(&self) -> State<i32> {
        self.trace.borrow().locals.last().unwrap().count
    }
    fn has(&self, label: &str) -> bool {
        self.source
            .adapter
            .placed
            .iter()
            .any(|p| p.paint.text.as_deref() == Some(label))
    }
}
fn program(records: Records) -> (Regions, State<Router<Page>>, State<i32>) {
    let mut states = None;
    let app = Regions::new(|app| {
        let router = app.persisted_state(1, Router::<Page>::default);
        let shared = app.state(|| 0);
        let records = app.state(|| records);
        states = Some((router, shared));
        app.root(
            (router, shared, records),
            |cx, (router, shared, records)| {
                text(cx, "Static shell");
                outlet(
                    cx,
                    router.handle(),
                    (records.handle(), shared.handle(), router.handle()),
                    page,
                );
            },
        );
    });
    let (router, shared) = states.unwrap();
    (app, router, shared)
}
#[allow(clippy::type_complexity)]
fn page(
    cx: &mut UiContext<'_>,
    selected: Page,
    (records, shared, router): (
        Access<'_, Records>,
        Access<'_, i32>,
        Access<'_, Router<Page>>,
    ),
) {
    let count = cx.local_persisted_state(1, || 0_i32);
    {
        let records = records.read();
        let mut records = records.borrow_mut();
        records.builds.push(selected);
        records.locals.push(Local {
            page: selected,
            count,
        });
    }
    text(cx, selected.label());
    text_empty(cx).rx((count,), |node, (count,)| {
        node.text(format!("Count {}", *count.read()));
    });
    text_empty(cx).rx((shared.handle(),), |node, (shared,)| {
        node.text(format!("Shared {}", *shared.read()));
    });
    button(cx)
        .text("Increment")
        .on_click((count,), |(count,)| count.update(|v| *v += 1));
    button(cx)
        .text("Next page")
        .on_click((router.handle(),), |(router,)| {
            router.update(|router| {
                router.go(if router.current() == Page::Home {
                    Page::Editor
                } else {
                    Page::Home
                })
            });
        });
    if selected == Page::Editor {
        let draft = cx.local_state(TextBuffer::new);
        cx.region((draft,), |cx, (draft,)| {
            text_input(cx, "Draft", &draft.read()).bind(draft.handle());
        });
    }
}
#[test]
fn shared_captures_update_properties_without_rebuilding_outlet_or_page() {
    let mut app = Harness::new();
    let before = app.trace.borrow().builds.len();
    let runs = app.source.view.region_runs();
    app.source.view.access(app.shared).set(42);
    app.frame();
    assert_eq!(app.trace.borrow().builds.len(), before);
    assert!(app.has("Shared 42"));
    for (id, runs) in runs {
        if app.source.engine.node(id).unwrap().kind != plico::Kind::Text {
            assert_eq!(
                app.source
                    .view
                    .region_runs()
                    .iter()
                    .find(|(e, _)| *e == id)
                    .unwrap()
                    .1,
                runs
            );
        }
    }
}
#[test]
fn same_route_keeps_local_state_and_changed_route_unmounts_old_owner() {
    let mut app = Harness::new();
    let old = app.local();
    let sender = app.source.view.sender(old);
    app.source.view.access(old).set(7);
    app.frame();
    app.navigate(Page::Home);
    assert_eq!(*app.source.view.access(old).read(), 7);
    app.source.request_rebuild();
    app.frame();
    assert_eq!(*app.source.view.access(old).read(), 7);
    app.navigate(Page::Editor);
    assert!(app.source.view.try_access(old).is_none());
    assert_eq!(sender.send(99), Err(99));
    assert_eq!(app.trace.borrow().locals.last().unwrap().page, Page::Editor);
    assert!(app.has("Count 0"));
    app.source.view.access(app.router).update(|r| {
        r.back();
    });
    app.frame();
    assert!(app.has("Home"));
    assert!(app.has("Count 0"));
    assert!(app.source.view.access(app.router).read().can_forward());
    app.source.view.access(app.router).update(|r| {
        r.forward();
    });
    app.frame();
    assert!(app.has("Editor"));
    assert!(app.has("Count 0"));
}
#[test]
fn queued_child_update_and_host_result_cannot_outlive_route_removal() {
    let mut app = Harness::new();
    app.navigate(Page::Editor);
    let old = app.local();
    let sender = app.source.view.sender(old);
    let tree = app.source.adapter.semantics(&app.source.engine, 0);
    let input = tree
        .node_by_role(SemanticRole::TextInput, "Draft", None)
        .unwrap()
        .id;
    let increment = tree
        .node_by_role(SemanticRole::Button, "Increment", None)
        .unwrap()
        .id;
    assert!(
        app.source
            .handle_semantic_action(SemanticAction::Focus { target: input })
    );
    app.frame();
    assert!(app.source.ime_state().is_some());
    sender.send(88).unwrap();
    app.source.view.access(old).set(12);
    app.source
        .view
        .access(app.router)
        .update(|r| r.go(Page::Home));
    app.frame();
    assert!(app.source.view.try_access(old).is_none());
    assert!(app.source.ime_state().is_none());
    assert!(
        !app.source
            .handle_semantic_action(SemanticAction::Click { target: increment })
    );
    assert!(
        !app.source
            .handle_semantic_action(SemanticAction::Focus { target: input })
    );
    assert_eq!(sender.send(100), Err(100));
    app.navigate(Page::Editor);
    assert!(app.has("Count 0"));
}
#[test]
fn page_navigation_event_updates_outlet_without_rebuilding_static_root() {
    let mut app = Harness::new();
    let root = app
        .source
        .engine
        .find(app.source.engine.root(), "regions-root")
        .unwrap();
    let runs = app
        .source
        .view
        .region_runs()
        .iter()
        .find(|(e, _)| *e == root)
        .unwrap()
        .1;
    let next = app
        .source
        .adapter
        .semantics(&app.source.engine, 0)
        .node_by_role(SemanticRole::Button, "Next page", None)
        .unwrap()
        .id;
    assert!(
        app.source
            .handle_semantic_action(SemanticAction::Click { target: next })
    );
    app.frame();
    assert!(app.has("Editor"));
    assert_eq!(
        app.source
            .view
            .region_runs()
            .iter()
            .find(|(e, _)| *e == root)
            .unwrap()
            .1,
        runs
    );
}
#[test]
fn repeated_navigation_has_bounded_live_nodes_and_active_page_checkpoint_restores() {
    let mut app = Harness::new();
    app.navigate(Page::Editor);
    let live = app.source.experiment_live_counts();
    let region_count = app.source.view.region_runs().len();
    for _ in 0..20 {
        app.navigate(Page::Home);
        app.navigate(Page::Editor);
    }
    assert_eq!(app.source.experiment_live_counts(), live);
    assert_eq!(app.source.view.region_runs().len(), region_count);
    app.source.view.access(app.local()).set(17);
    app.frame();
    let snapshot = app.source.view.checkpoint().unwrap();
    let old = app.local();
    let records = Records::default();
    let (mut restored, router, shared) = program(Rc::clone(&records));
    restored.restore_checkpoint(snapshot);
    let mut app = Harness {
        source: Source::new(restored),
        text: TextContext::new(),
        router,
        shared,
        trace: records,
    };
    app.frame();
    assert_eq!(
        app.source.view.access(router).read().current(),
        Page::Editor
    );
    assert!(app.has("Count 17"));
    assert!(app.source.view.try_access(old).is_none());
}
#[test]
fn parameterized_route_keys_control_remount_or_parameter_reuse() {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Document {
        Item(u64),
    }
    fn fixture(
        per_item: bool,
        records: Rc<RefCell<Vec<State<i32>>>>,
    ) -> (Source<Regions>, State<Router<Document>>) {
        let mut state = None;
        let app = Regions::new(|app| {
            let router = app.state(|| Router::new(Document::Item(1)));
            let records = app.state(|| records);
            let per_item = app.state(|| per_item);
            state = Some(router);
            app.root(
                (router, records, per_item),
                |cx, (router, records, per_item)| {
                    let key: fn(Document) -> Key = if *per_item.read() {
                        |Document::Item(id)| Key::Number(id)
                    } else {
                        |_| Key::Static("document")
                    };
                    outlet_keyed(
                        cx,
                        router.handle(),
                        (records.handle(),),
                        key,
                        |cx, Document::Item(id), (records,)| {
                            let local = cx.local_state(|| 0_i32);
                            records.read().borrow_mut().push(local);
                            text!(cx, "Document {id}");
                            text_empty(cx).rx((local,), |node, (local,)| {
                                node.text(local.read().to_string());
                            });
                        },
                    );
                },
            );
        });
        (Source::new(app), state.unwrap())
    }
    for remount in [false, true] {
        let records = Rc::new(RefCell::new(vec![]));
        let (mut source, router) = fixture(remount, Rc::clone(&records));
        let mut text = TextContext::new();
        source.experiment_prepare_without_scene(&mut text, 600, 600);
        let old = records.borrow()[0];
        source.view.access(old).set(9);
        source
            .view
            .access(router)
            .update(|r| r.go(Document::Item(2)));
        source.experiment_prepare_without_scene(&mut text, 600, 600);
        assert!(source.last_error.is_none(), "{:?}", source.last_error);
        assert!(
            source
                .adapter
                .placed
                .iter()
                .any(|p| p.paint.text.as_deref() == Some("Document 2"))
        );
        assert_eq!(source.view.try_access(old).is_none(), remount);
        let current = *records.borrow().last().unwrap();
        assert_eq!(
            *source.view.access(current).read(),
            if remount { 0 } else { 9 }
        );
    }
}

#[test]
fn sibling_outlets_have_independent_page_owners() {
    type Counts = Rc<RefCell<Vec<State<i32>>>>;
    fn render(cx: &mut UiContext<'_>, _: Page, (records,): (Access<'_, Counts>,)) {
        let count = cx.local_state(|| 0_i32);
        records.read().borrow_mut().push(count);
        text_empty(cx).rx((count,), |node, (count,)| {
            node.text(count.read().to_string());
        });
    }
    let records = Counts::default();
    let mut state = None;
    let view = Regions::new(|app| {
        let router = app.state(Router::<Page>::default);
        let captures = app.state(|| Rc::clone(&records));
        state = Some(router);
        app.root((router, captures), |cx, (router, captures)| {
            outlet(cx, router.handle(), (captures.handle(),), render);
            outlet(cx, router.handle(), (captures.handle(),), render);
            for key in 0..2_u64 {
                cx.keyed(key, |cx| {
                    outlet(cx, router.handle(), (captures.handle(),), render)
                });
            }
        });
    });
    let mut source = Source::new(view);
    let mut text = TextContext::new();
    source.experiment_prepare_without_scene(&mut text, 600, 600);
    let old = records.borrow().clone();
    assert_eq!(old.len(), 4);
    source.view.access(old[0]).set(9);
    source.experiment_prepare_without_scene(&mut text, 600, 600);
    assert_eq!(*source.view.access(old[1]).read(), 0);
    source
        .view
        .access(state.unwrap())
        .update(|r| r.go(Page::Editor));
    source.experiment_prepare_without_scene(&mut text, 600, 600);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    assert!(source.view.try_access(old[0]).is_none());
    assert!(source.view.try_access(old[1]).is_none());
    assert_eq!(records.borrow().len(), 8);
}

#[test]
fn parent_route_unmount_recursively_disposes_nested_outlet_and_its_state() {
    type Nested = Rc<RefCell<Vec<(State<Router<Page>>, State<i32>)>>>;
    let records = Nested::default();
    let mut outer = None;
    let view = Regions::new(|app| {
        let router = app.state(Router::<Page>::default);
        outer = Some(router);
        let records = app.state(|| Rc::clone(&records));
        app.root((router, records), |cx, (router, records)| {
            outlet(
                cx,
                router.handle(),
                (records.handle(),),
                |cx, route, (records,)| {
                    if route == Page::Home {
                        text(cx, "Outer Home");
                    } else {
                        let inner = cx.local_state(Router::<Page>::default);
                        outlet(
                            cx,
                            inner,
                            (inner, records.handle()),
                            |cx, page, (inner, records)| {
                                let count = cx.local_state(|| 0_i32);
                                records.read().borrow_mut().push((inner.handle(), count));
                                text(cx, page.label());
                                text_empty(cx).rx((count,), |node, (count,)| {
                                    node.text(count.read().to_string());
                                });
                            },
                        );
                    }
                },
            );
        });
    });
    let mut source = Source::new(view);
    let mut text = TextContext::new();
    source.experiment_prepare_without_scene(&mut text, 600, 600);
    let baseline = source.experiment_live_counts();
    source
        .view
        .access(outer.unwrap())
        .update(|r| r.go(Page::Editor));
    source.experiment_prepare_without_scene(&mut text, 600, 600);
    let (inner, old) = records.borrow()[0];
    source.view.access(inner).update(|r| r.go(Page::Editor));
    source.experiment_prepare_without_scene(&mut text, 600, 600);
    assert!(source.view.try_access(old).is_none());
    let (_, current) = *records.borrow().last().unwrap();
    let sender = source.view.sender(current);
    source.view.access(current).set(8);
    source
        .view
        .access(outer.unwrap())
        .update(|r| r.go(Page::Home));
    source.experiment_prepare_without_scene(&mut text, 600, 600);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    assert!(source.view.try_access(inner).is_none());
    assert!(source.view.try_access(current).is_none());
    assert_eq!(sender.send(99), Err(99));
    assert_eq!(source.experiment_live_counts(), baseline);
}

#[test]
fn non_subscribed_captures_still_validate_state_ownership() {
    let mut foreign_state = None;
    let _foreign = Regions::new(|app| {
        foreign_state = Some(app.state(|| 1_i32));
        app.root((), |cx, ()| {
            text(cx, "Foreign owner");
        });
    });
    let app = Regions::new(|app| {
        let router = app.state(Router::<Page>::default);
        let capture = app.state(|| foreign_state.unwrap());
        app.root((router, capture), |cx, (router, capture)| {
            outlet(
                cx,
                router.handle(),
                (*capture.read(),),
                |cx, _, (value,)| {
                    text!(cx, "{}", *value.read());
                },
            );
        });
    });
    let mut source = Source::new(app);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source.try_declare()))
        .unwrap_err();
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap();
    assert!(message.contains("foreign state owner"), "{message}");
}

#[test]
fn fill_outlet_and_watch_allocate_the_remaining_page_height() {
    let app = Regions::new(|app| {
        let router = app.state(Router::<Page>::default);
        app.root((router,), |cx, (router,)| {
            let _ = column(cx).width_percent(100.).height_percent(100.)
                + |cx| {
                    text(cx, "Shell header");
                    outlet_fill(cx, router.handle(), (), |cx, _, ()| {
                        cx.region_fill((), |cx, ()| {
                            let _ = column(cx).width_percent(100.).height_percent(100.)
                                + |cx| {
                                    text(cx, "Page header");
                                    column(cx).height(0.).grow(1.).end();
                                    text(cx, "Page footer");
                                };
                        });
                    });
                };
        });
    });
    let mut source = Source::new(app);
    let mut text = TextContext::new();
    source.experiment_prepare_without_scene(&mut text, 700, 700);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    let footer = source
        .adapter
        .placed
        .iter()
        .find(|placed| placed.paint.text.as_deref() == Some("Page footer"))
        .expect("page footer is placed");
    assert!(
        footer.rect[1] > 650.,
        "footer did not reach the remaining page height: {:?}",
        footer.rect
    );
    assert!(
        footer.rect[3] <= 700.,
        "footer escaped the viewport: {:?}",
        footer.rect
    );
}
