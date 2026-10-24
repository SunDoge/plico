//! Run under Xvfb; exercises the real native widgets, not a mock projection.
use fltk_native::{app, draw, group::Group, prelude::*, window::Window};
use plico_backend_settings::Settings;
use plico_core::{Engine, Entity, Kind};
use plico_fltk::{Adapter, Event};
use std::{io::Write, time::Instant};
fn named(engine: &Engine, kind: Kind, name: &str) -> Entity {
    fn visit(engine: &Engine, e: Entity, kind: Kind, name: &str) -> Option<Entity> {
        let n = engine.node(e)?;
        if n.kind == kind && engine.accessible_name(e).as_deref() == Some(name) {
            return Some(e);
        }
        n.children
            .iter()
            .find_map(|c| visit(engine, *c, kind, name))
    }
    visit(engine, engine.root(), kind, name).unwrap_or_else(|| panic!("missing {kind:?} {name}"))
}
fn capture(window: &mut Window, suffix: &str) {
    if let Ok(base) = std::env::var("PLICO_FLTK_PROBE_OUT") {
        window.redraw();
        // X11 map/configure notifications may arrive after the first check.
        // Capture only after giving the native window a chance to become ready.
        let image = (0..20)
            .find_map(|_| {
                app::wait_for(0.01).unwrap();
                app::flush();
                draw::capture_window(window).ok()
            })
            .expect("window capture after map/configure");
        let path = format!("{base}-{suffix}.ppm");
        let mut file = std::fs::File::create(path).unwrap();
        write!(file, "P6\n{} {}\n255\n", image.data_w(), image.data_h()).unwrap();
        file.write_all(&image.to_rgb_data()).unwrap();
    }
}
#[plico_core::component]
fn scroll_scene(cx: &mut plico_core::UiContext, reversed: bool) {
    use plico_core::prelude::*;
    column(cx)
        .key("scroll")
        .width(300.)
        .height(160.)
        .scroll_y(true)
        .children(|cx| {
            let mut ids: Vec<u64> = (0..30).collect();
            if reversed {
                ids.reverse();
            }
            for id in ids {
                text!(cx, "Row {id}").key(id).height(28.).min_height(28.);
            }
        });
}
fn frame_at(engine: &mut Engine, view: &mut Settings, adapter: &mut Adapter, millis: u64) {
    let now = std::time::Duration::from_millis(millis);
    engine.set_time(plico_core::FrameTime {
        now,
        delta: now.saturating_sub(engine.time().now),
    });
    engine.declare_until_settled(view).unwrap();
    adapter.sync(engine).unwrap();
}
fn scroll_entity(engine: &Engine, e: Entity) -> Option<Entity> {
    let node = engine.node(e)?;
    if node.style.scroll_y {
        return Some(e);
    }
    node.children.iter().find_map(|e| scroll_entity(engine, *e))
}
fn main() {
    let start = Instant::now();
    let _app = app::App::default().with_scheme(app::Scheme::Gtk);
    let mut window = Window::new(0, 0, 680, 700, "Plico FLTK probe");
    let root = Group::new(0, 0, 680, 700, "");
    root.end();
    window.end();
    window.show();
    let (tx, rx) = app::channel::<Event>();
    let mut adapter = Adapter::new(root, tx);
    let mut engine = Engine::default();
    engine.set_viewport(680., 700.).unwrap();
    let mut view = Settings::default();
    view.router
        .replace(plico_backend_settings::Page::Preferences);
    engine.declare_until_settled(&mut view).unwrap();
    adapter.sync(&engine).unwrap();
    app::check();
    app::flush();
    println!(
        "first_frame_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
    capture(&mut window, "light");
    let input = named(&engine, Kind::TextInput, "Workspace name");
    let pointer = adapter.native_widget(input).unwrap().as_widget_ptr();
    let edit = "Native input: 中文 Agjp";
    let mut native_input = adapter.native_input(input).unwrap();
    native_input.set_value(edit);
    native_input.do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    engine.declare_until_settled(&mut view).unwrap();
    adapter.sync(&engine).unwrap();
    assert_eq!(view.name.text().as_deref(), Some(edit));
    assert_eq!(
        adapter.native_widget(input).unwrap().as_widget_ptr(),
        pointer
    );
    // Call the native button callback, then deliver its queued identity.
    let save = named(&engine, Kind::ButtonBase, "Save changes");
    adapter.native_widget(save).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    engine.declare_until_settled(&mut view).unwrap();
    adapter.sync(&engine).unwrap();
    assert_eq!(view.saves, 1);
    assert!(view.saved.contains(edit));
    let disabled = named(&engine, Kind::ButtonBase, "Disabled action");
    assert!(!adapter.native_widget(disabled).unwrap().active());
    assert!(!Adapter::dispatch(&mut engine, Event::Activate(disabled)));
    let advanced = named(&engine, Kind::Checkbox, "Show advanced options");
    let mut old_nested = None;
    for _ in 0..20 {
        adapter.native_widget(advanced).unwrap().do_callback();
        assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
        engine.declare_until_settled(&mut view).unwrap();
        adapter.sync(&engine).unwrap();
        assert!(view.advanced);
        let nested = named(&engine, Kind::ButtonBase, "Nested action");
        old_nested = Some(nested);
        adapter.native_widget(advanced).unwrap().do_callback();
        assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
        engine.declare_until_settled(&mut view).unwrap();
        adapter.sync(&engine).unwrap();
        assert!(!view.advanced);
        assert!(adapter.native_widget(nested).is_none());
        assert!(!Adapter::dispatch(&mut engine, Event::Activate(nested)));
        assert_eq!(
            adapter.native_widget(input).unwrap().as_widget_ptr(),
            pointer
        );
    }
    assert!(old_nested.is_some());
    view.dark = true;
    engine.declare_until_settled(&mut view).unwrap();
    adapter.sync(&engine).unwrap();
    capture(&mut window, "dark");
    window.resize(0, 0, 520, 760);
    // Adapter roots resize with their host; FLTK automatically resizes its group
    // when the group is marked resizable by an embedding application.
    // This probe uses a fresh target extent via the public root handle below.
    window.redraw();
    app::check();
    let root = engine.root();
    let mut widget = adapter.native_widget(root).unwrap();
    widget.resize(0, 0, 520, 760);
    engine.set_viewport(520., 760.).unwrap();
    engine.declare_until_settled(&mut view).unwrap();
    adapter.sync(&engine).unwrap();
    let bounds = adapter.bounds(input).unwrap();
    assert!(bounds[0] >= 0 && bounds[0] + bounds[2] <= 520);
    capture(&mut window, "narrow");
    engine.declare_until_settled(&mut view).unwrap();
    adapter.sync(&engine).unwrap();
    assert_eq!(adapter.stats.created, 0);
    assert_eq!(adapter.stats.removed, 0);
    assert_eq!(adapter.stats.geometry_writes, 0);
    assert_eq!(adapter.stats.property_writes, 0);
    println!("retained_input=true mount_remove_cycles=20 stale_events_rejected=true idle_writes=0");
    let start = Instant::now();
    for _ in 0..100 {
        engine.declare_until_settled(&mut view).unwrap();
        adapter.sync(&engine).unwrap();
    }
    println!(
        "forced_declare_sync_us={:.3}",
        start.elapsed().as_secs_f64() * 1e6 / 100.
    );
    // Actual router buttons and clock-driven UI values, using the same view.
    window.resize(0, 0, 1100, 820);
    adapter
        .native_widget(engine.root())
        .unwrap()
        .resize(0, 0, 1100, 820);
    engine.set_viewport(1100., 820.).unwrap();
    view.dark = false;
    let nav = named(&engine, Kind::ButtonBase, "Overview");
    let nav_pointer = adapter.native_widget(nav).unwrap().as_widget_ptr();
    adapter.native_widget(nav).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 500);
    frame_at(&mut engine, &mut view, &mut adapter, 800);
    assert_eq!(
        view.router.current(),
        plico_backend_settings::Page::Overview
    );
    let metrics: Vec<_> = ["PROJECTS", "STORAGE", "SYNC HEALTH"]
        .map(|name| adapter.bounds(named(&engine, Kind::Text, name)).unwrap())
        .to_vec();
    assert!(
        metrics
            .windows(2)
            .all(|r| r[0][1] == r[1][1] && r[0][0] < r[1][0])
    );
    capture(&mut window, "overview");
    let details = named(&engine, Kind::ButtonBase, "Details");
    adapter.native_widget(details).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 810);
    assert!(engine.changes().contains(plico_core::Changes::ANIMATE));
    frame_at(&mut engine, &mut view, &mut adapter, 940);
    assert!(view.inspector_width() > 0. && view.inspector_width() < 248.);
    capture(&mut window, "details-mid");
    frame_at(&mut engine, &mut view, &mut adapter, 1100);
    assert_eq!(view.inspector_width(), 248.);
    assert!(!engine.changes().contains(plico_core::Changes::ANIMATE));
    capture(&mut window, "details");
    let collapse = named(&engine, Kind::ButtonBase, "Toggle sidebar");
    adapter.native_widget(collapse).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 1110);
    frame_at(&mut engine, &mut view, &mut adapter, 1240);
    assert!(view.sidebar_width() > 72. && view.sidebar_width() < 200.);
    frame_at(&mut engine, &mut view, &mut adapter, 1400);
    assert_eq!(view.sidebar_width(), 72.);
    assert_eq!(
        adapter.native_widget(nav).unwrap().as_widget_ptr(),
        nav_pointer
    );
    let run = named(&engine, Kind::ButtonBase, "Run preview");
    adapter.native_widget(run).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 1500);
    frame_at(&mut engine, &mut view, &mut adapter, 2700);
    assert!((view.progress() - 0.5).abs() < 0.001);
    capture(&mut window, "progress-mid");
    frame_at(&mut engine, &mut view, &mut adapter, 3900);
    assert_eq!(view.progress(), 1.);
    assert!(!view.animation_active());
    assert!(!engine.changes().contains(plico_core::Changes::ANIMATE));
    let activity = named(&engine, Kind::ButtonBase, "Activity");
    adapter.native_widget(activity).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 4000);
    frame_at(&mut engine, &mut view, &mut adapter, 4300);
    let header = named(&engine, Kind::Text, "EVENT");
    let header_y = adapter.bounds(header).unwrap()[1];
    let heading_x = adapter
        .bounds(named(&engine, Kind::Text, "WORKSPACE"))
        .unwrap()[0];
    let heading_state = adapter.bounds(named(&engine, Kind::Text, "STATE")).unwrap()[0];
    assert_eq!(
        heading_x,
        adapter
            .bounds(named(&engine, Kind::Text, "Desktop client"))
            .unwrap()[0]
    );
    assert_eq!(
        heading_state,
        adapter
            .bounds(named(&engine, Kind::Text, "Complete"))
            .unwrap()[0]
    );
    let first_event = named(&engine, Kind::Text, "Sync checkpoint 01");
    let event_pointer = adapter.native_widget(first_event).unwrap().as_widget_ptr();
    let scroll_id = scroll_entity(&engine, engine.root()).unwrap();
    let mut scroll = adapter.native_scroll(scroll_id).unwrap();
    app::check();
    app::flush();
    scroll.scroll_to(0, 120);
    frame_at(&mut engine, &mut view, &mut adapter, 4400);
    assert_eq!(adapter.bounds(header).unwrap()[1], header_y);
    assert_eq!(scroll.yposition(), 120);
    let reverse = named(&engine, Kind::ButtonBase, "Reverse order");
    adapter.native_widget(reverse).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 4500);
    assert_eq!(
        adapter.native_widget(first_event).unwrap().as_widget_ptr(),
        event_pointer
    );
    capture(&mut window, "activity");
    let preferences = named(&engine, Kind::ButtonBase, "Preferences");
    adapter.native_widget(preferences).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 4600);
    assert_eq!(view.name.text().as_deref(), Some(edit));
    assert_eq!(
        adapter
            .native_input(named(&engine, Kind::TextInput, "Workspace name"))
            .unwrap()
            .value(),
        edit
    );
    let back = named(&engine, Kind::ButtonBase, "Back");
    adapter.native_widget(back).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 4700);
    assert_eq!(
        view.router.current(),
        plico_backend_settings::Page::Activity
    );
    let forward = named(&engine, Kind::ButtonBase, "Forward");
    adapter.native_widget(forward).unwrap().do_callback();
    assert!(Adapter::dispatch(&mut engine, rx.recv().unwrap()));
    frame_at(&mut engine, &mut view, &mut adapter, 4800);
    assert_eq!(
        view.router.current(),
        plico_backend_settings::Page::Preferences
    );
    view.compact = false;
    view.inspector = false;
    view.reduced_motion = true;
    frame_at(&mut engine, &mut view, &mut adapter, 5000);
    assert_eq!(view.sidebar_width(), 200.);
    assert!(!view.animation_active());
    assert!(!engine.changes().contains(plico_core::Changes::ANIMATE));
    println!(
        "router_back_forward=true grid_aligned=true finite_motion=true reduced_motion=true route_state_preserved=true"
    );
    engine
        .frame_declarations(|cx| scroll_scene(cx, false))
        .unwrap();
    adapter.sync(&engine).unwrap();
    app::check();
    app::flush();
    let scroll_id = engine.node(engine.root()).unwrap().children[0];
    let row_id = engine.node(scroll_id).unwrap().children[0];
    let row_pointer = adapter.native_widget(row_id).unwrap().as_widget_ptr();
    let mut scroll = adapter.native_scroll(scroll_id).unwrap();
    let y = adapter.bounds(row_id).unwrap()[1];
    scroll.scroll_to(0, 90);
    app::check();
    assert_eq!(adapter.bounds(row_id).unwrap()[1], y - 90);
    engine
        .frame_declarations(|cx| scroll_scene(cx, false))
        .unwrap();
    adapter.sync(&engine).unwrap();
    assert_eq!(scroll.yposition(), 90);
    assert_eq!(adapter.bounds(row_id).unwrap()[1], y - 90);
    engine
        .frame_declarations(|cx| scroll_scene(cx, true))
        .unwrap();
    adapter.sync(&engine).unwrap();
    assert_eq!(
        adapter.native_widget(row_id).unwrap().as_widget_ptr(),
        row_pointer
    );
    assert!(adapter.bounds(row_id).unwrap()[1] > y + 500);
    println!("scroll_offset_preserved=true keyed_reorder_retains_widgets=true");
    // Reusing an explicit identity for another native kind remounts safely.
    for kind in [Kind::Text, Kind::Button, Kind::Text] {
        engine
            .frame_declarations(|cx| {
                cx.element(
                    plico_core::Key::Static("kind-change"),
                    kind,
                    plico_core::Style::default(),
                    "Changing kind",
                    |_| {},
                );
            })
            .unwrap();
        adapter.sync(&engine).unwrap();
    }
    engine
        .frame_declarations(|cx| {
            cx.element(
                plico_core::Key::Static("unsupported"),
                Kind::Shader,
                plico_core::Style::default(),
                "",
                |_| {},
            );
        })
        .unwrap();
    assert!(matches!(
        adapter.sync(&engine),
        Err(plico_fltk::Error::Unsupported { kind: Kind::Shader })
    ));
    println!("kind_remount_safe=true unsupported_nodes_rejected=true");
    window.hide();
}
