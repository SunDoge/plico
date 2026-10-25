use gallery::{Gallery, Page};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point, PointerButton, PointerEvent,
    PointerPhase, SemanticAction, SemanticRole, SemanticToggleState, TextContext, UiEvent,
};
use plico_vello::{Source, View};
fn frame(s: &mut Source<Gallery>, tcx: &mut TextContext, width: u32) {
    s.build_frame(tcx, width, 780);
}
fn named(s: &Source<Gallery>, role: SemanticRole, label: &str) -> plico_renderer::SemanticNode {
    s.adapter
        .semantics(&s.engine, 0)
        .node_by_role(role, label, None)
        .unwrap()
        .clone()
}
fn pointer(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y },
        button: Some(PointerButton::Primary),
        buttons: if phase == PointerPhase::Down { 1 } else { 0 },
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    })
}
fn click(s: &mut Source<Gallery>, tcx: &mut TextContext, role: SemanticRole, label: &str) {
    let b = named(s, role, label).bounds;
    let (x, y) = (((b[0] + b[2]) / 2.) as f64, ((b[1] + b[3]) / 2.) as f64);
    s.handle_event(pointer(PointerPhase::Down, x, y));
    s.prepare_for_event(tcx);
    s.handle_event(pointer(PointerPhase::Up, x, y));
    s.prepare_for_event(tcx);
    frame(s, tcx, 920);
}
fn key(key: &str, shift: bool) -> UiEvent {
    UiEvent::Key(KeyEvent {
        phase: KeyPhase::Down,
        key: key.into(),
        key_without_modifiers: key.into(),
        code: key.into(),
        text: None,
        text_with_all_modifiers: None,
        location: KeyLocation::Standard,
        modifiers: if shift {
            Modifiers::SHIFT
        } else {
            Modifiers::empty()
        },
        repeat: false,
        synthetic: false,
    })
}
#[test]
fn real_pointer_counter_reset_and_disabled() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    click(&mut s, &mut tcx, SemanticRole::Button, "Increment");
    assert_eq!(s.view.count, 1);
    click(&mut s, &mut tcx, SemanticRole::Button, "Disabled");
    assert_eq!(s.view.count, 1);
    click(&mut s, &mut tcx, SemanticRole::Button, "Reset");
    assert_eq!(s.view.count, 0);
}
#[test]
fn controlled_toggles_publish_text_style_and_theme_same_frame() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    click(&mut s, &mut tcx, SemanticRole::Switch, "Notifications");
    assert!(s.view.notifications);
    assert_eq!(
        named(&s, SemanticRole::Switch, "Notifications")
            .states
            .checked,
        Some(SemanticToggleState::On)
    );
    named(&s, SemanticRole::Label, "Notifications are enabled");
    click(
        &mut s,
        &mut tcx,
        SemanticRole::CheckBox,
        "Save automatically",
    );
    assert!(s.view.autosave);
    click(&mut s, &mut tcx, SemanticRole::CheckBox, "Large text");
    assert!(s.view.large);
    click(&mut s, &mut tcx, SemanticRole::Switch, "Dark theme");
    assert!(s.view.dark);
    assert_eq!(s.engine.theme(), s.view.theme());
}
#[test]
fn unchanged_frame_reuses_backend_and_writes_nothing() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    let id = s.adapter.backend_id(s.engine.root());
    frame(&mut s, &mut tcx, 920);
    assert_eq!(s.adapter.backend_id(s.engine.root()), id);
    let b = s.adapter.stats;
    assert_eq!(
        (
            b.created,
            b.removed,
            b.style_writes,
            b.paint_writes,
            b.child_writes
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(s.declaration_stats.measure_calls, 0);
    assert_eq!(s.declaration_stats.layout_runs, 0);
}
#[test]
fn theme_changes_paint_without_layout_style_writes() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    s.view.dark = true;
    frame(&mut s, &mut tcx, 920);
    assert_eq!(s.adapter.stats.style_writes, 0);
    assert!(s.adapter.stats.paint_writes > 0);
}
#[test]
fn tab_space_shift_tab_and_disabled_skipping() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    let dark = named(&s, SemanticRole::Switch, "Dark theme").id;
    for _ in 0..12 {
        s.handle_event(key("Tab", false));
        if s.adapter.semantics(&s.engine, 0).focus == Some(dark) {
            break;
        }
    }
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(dark));
    s.handle_event(key(" ", false));
    frame(&mut s, &mut tcx, 920);
    assert!(s.view.dark);
    s.handle_event(key("Tab", false));
    s.handle_event(key(" ", false));
    frame(&mut s, &mut tcx, 920);
    assert!(s.view.large);
    let increment = named(&s, SemanticRole::Button, "Increment").id;
    for _ in 0..12 {
        s.handle_event(key("Tab", false));
        if s.adapter.semantics(&s.engine, 0).focus == Some(increment) {
            break;
        }
    }
    assert_eq!(s.adapter.semantics(&s.engine, 0).focus, Some(increment));
    s.handle_event(key("Enter", false));
    frame(&mut s, &mut tcx, 920);
    assert_eq!(s.view.count, 1);
    s.handle_event(key("Tab", false));
    s.handle_event(key("Tab", false));
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(named(&s, SemanticRole::Button, "Quick settings").id)
    );
    s.handle_event(key("Tab", false));
    let snapshot = s.adapter.semantics(&s.engine, 0);
    assert_eq!(
        snapshot.focus,
        Some(named(&s, SemanticRole::Switch, "Notifications").id)
    );
    s.handle_event(key("Tab", true));
    assert_eq!(
        s.adapter.semantics(&s.engine, 0).focus,
        Some(named(&s, SemanticRole::Button, "Quick settings").id)
    );
    s.handle_event(key("Tab", true));
    let snapshot = s.adapter.semantics(&s.engine, 0);
    assert_eq!(
        snapshot.focus,
        Some(named(&s, SemanticRole::Button, "Reset").id)
    );
}
#[test]
fn release_outside_cancel_and_secondary_do_not_activate() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    let b = named(&s, SemanticRole::Button, "Increment").bounds;
    let x = (b[0] + 2.) as f64;
    let y = (b[1] + 2.) as f64;
    s.handle_event(pointer(PointerPhase::Down, x, y));
    s.handle_event(pointer(PointerPhase::Up, 0., 0.));
    frame(&mut s, &mut tcx, 920);
    assert_eq!(s.view.count, 0);
    s.handle_event(pointer(PointerPhase::Down, x, y));
    s.handle_event(pointer(PointerPhase::Cancel, x, y));
    s.handle_event(pointer(PointerPhase::Up, x, y));
    frame(&mut s, &mut tcx, 920);
    assert_eq!(s.view.count, 0);
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        let UiEvent::Pointer(mut p) = pointer(phase, x, y) else {
            unreachable!()
        };
        p.button = Some(PointerButton::Secondary);
        s.handle_event(UiEvent::Pointer(p));
    }
    frame(&mut s, &mut tcx, 920);
    assert_eq!(s.view.count, 0);
}
#[test]
fn semantic_actions_route_through_controlled_state() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    let target = named(&s, SemanticRole::Switch, "Notifications").id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target }));
    frame(&mut s, &mut tcx, 920);
    assert!(s.view.notifications);
    let target = named(&s, SemanticRole::Button, "Disabled").id;
    assert!(!s.handle_semantic_action(SemanticAction::Click { target }));
}
#[test]
fn real_layout_fits_normal_and_minimum_viewports() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    for width in [640, 920] {
        for large in [false, true] {
            s.view.large = large;
            frame(&mut s, &mut tcx, width);
            for p in &s.adapter.placed {
                let mut r = p.rect;
                assert!(r.iter().all(|n| n.is_finite()));
                // Scroll content may extend beyond the viewport; its visible
                // intersection must remain bounded by the scrollport.
                if let Some(clip) = p.clip {
                    r = [
                        r[0].max(clip[0]),
                        r[1].max(clip[1]),
                        r[2].min(clip[2]),
                        r[3].min(clip[3]),
                    ];
                    if r[0] >= r[2] || r[1] >= r[3] {
                        continue;
                    }
                }
                assert!(
                    r[0] >= -0.5 && r[1] >= -0.5 && r[2] <= width as f32 + 0.5 && r[3] <= 780.5,
                    "{r:?}"
                );
            }
            let items = s.adapter.semantics(&s.engine, 0);
            for n in items.nodes.iter().filter(|n| {
                matches!(
                    n.role,
                    SemanticRole::Button | SemanticRole::Switch | SemanticRole::CheckBox
                )
            }) {
                assert!(
                    n.bounds[3] - n.bounds[1] >= s.view.theme().metrics.control_height as f32 - 0.1
                );
            }
        }
    }
}

#[test]
fn retained_backend_handles_reorder_remove_and_remount() {
    use plico_core::{Engine, UiContext};
    use plico_vello::Adapter;
    let mut e = Engine::default();
    let mut a = Adapter::default();
    let mut tcx = TextContext::new();
    let draw = |ui: &mut UiContext<'_>, ids: &[u64]| {
        for id in ids {
            ui.text("item").key(*id);
        }
    };
    e.frame_declarations(|ui| draw(ui, &[1, 2])).unwrap();
    a.layout(&mut e, &mut tcx, 640, 780);
    let one = e.find(e.root(), 1u64).unwrap();
    let two = e.find(e.root(), 2u64).unwrap();
    let first = a.backend_id(one);
    let second = a.backend_id(two);
    e.frame_declarations(|ui| draw(ui, &[2, 1])).unwrap();
    a.layout(&mut e, &mut tcx, 640, 780);
    assert_eq!(a.backend_id(one), first);
    assert_eq!(a.backend_id(two), second);
    assert_eq!(a.stats.created, 0);
    assert_eq!(a.stats.child_writes, 1);
    e.frame_declarations(|ui| draw(ui, &[2])).unwrap();
    a.layout(&mut e, &mut tcx, 640, 780);
    assert_eq!(a.stats.removed, 1);
    assert!(a.backend_id(one).is_none());
    e.frame_declarations(|ui| draw(ui, &[2, 1])).unwrap();
    a.layout(&mut e, &mut tcx, 640, 780);
    let new_one = e.find(e.root(), 1u64).unwrap();
    assert_ne!(new_one, one);
    assert_ne!(a.backend_id(new_one), first);
    assert_eq!(a.backend_id(two), second);
}
#[test]
fn prepare_event_commits_new_geometry_before_next_pointer_event() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    let before = named(&s, SemanticRole::Label, "Nightly").bounds;
    let target = named(&s, SemanticRole::CheckBox, "Large text").id;
    s.handle_semantic_action(SemanticAction::Click { target });
    s.prepare_for_event(&mut tcx);
    let after = named(&s, SemanticRole::Label, "Nightly").bounds;
    assert!(after[3] - after[1] > before[3] - before[1]);
}

#[test]
fn declaration_errors_preserve_last_geometry_instead_of_panicking() {
    struct Invalid(bool);
    impl View for Invalid {
        fn show(&mut self, ui: &mut plico_core::UiContext<'_>) {
            ui.text("first").key("a");
            if self.0 {
                ui.text("second").key("a");
            }
        }
    }
    let mut source = Source::new(Invalid(false));
    let mut tcx = TextContext::new();
    let good = source.build_frame(&mut tcx, 640, 740);
    source.view.0 = true;
    let failed = source.build_frame(&mut tcx, 640, 740);
    assert_eq!(good.len(), failed.len());
    assert!(matches!(
        source.last_error,
        Some(plico_vello::SourceError::Declaration { .. })
    ));
    source.view.0 = false;
    source.build_frame(&mut tcx, 640, 740);
    assert!(source.last_error.is_none());
}

#[test]
fn expanded_motion_card_fits_minimum_viewport_and_large_text() {
    for width in [640, 920] {
        let mut source = Source::new(Gallery {
            router: plico_core::Router::new(Page::Motion),
            expanded: true,
            large: true,
            ..Default::default()
        });
        let mut tcx = TextContext::new();
        let nodes = source.build_frame(&mut tcx, width, 780);
        for node in nodes {
            assert!(
                node.rect[2] <= width as f32 + 0.5 && node.rect[3] <= 780.5,
                "{:?}",
                node.rect
            );
        }
    }
}

#[test]
fn typed_routes_unmount_pages_and_keep_application_state() {
    let mut s = Source::new(Gallery::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    assert!(s.view.router.show(Page::Workspace));
    click(&mut s, &mut tcx, SemanticRole::Button, "Controls");
    click(&mut s, &mut tcx, SemanticRole::Button, "Increment");
    click(&mut s, &mut tcx, SemanticRole::Switch, "Notifications");
    click(&mut s, &mut tcx, SemanticRole::Button, "Motion");
    assert!(s.view.router.show(Page::Motion));
    assert!(
        s.adapter
            .semantics(&s.engine, 0)
            .node_by_role(SemanticRole::Button, "Increment", None)
            .is_none()
    );
    click(&mut s, &mut tcx, SemanticRole::Button, "Controls");
    assert_eq!(s.view.count, 1);
    assert!(s.view.notifications);
    let restored: Gallery = serde_json::from_value(serde_json::to_value(&s.view).unwrap()).unwrap();
    assert_eq!(restored, s.view);
}

#[test]
fn all_gallery_routes_fit_minimum_and_regular_windows() {
    for width in [640, 920, 1120] {
        for page in [
            Page::Workspace,
            Page::Layout,
            Page::Controls,
            Page::Motion,
            Page::Lists,
        ] {
            let mut s = Source::new(Gallery {
                router: plico_core::Router::new(page),
                large: true,
                expanded: true,
                wrapped: true,
                release_details: true,
                ..Default::default()
            });
            let mut tcx = TextContext::new();
            s.build_frame(&mut tcx, width, 740);
            assert!(s.last_error.is_none());
            for n in &s.adapter.placed {
                let mut r = n.rect;
                if let Some(clip) = n.clip {
                    r = [
                        r[0].max(clip[0]),
                        r[1].max(clip[1]),
                        r[2].min(clip[2]),
                        r[3].min(clip[3]),
                    ];
                    if r[0] >= r[2] || r[1] >= r[3] {
                        continue;
                    }
                }
                assert!(
                    r.iter().all(|n| n.is_finite())
                        && r[0] >= -0.5
                        && r[1] >= -0.5
                        && r[2] <= width as f32 + 0.5
                        && r[3] <= 740.5,
                    "{page:?} {width} {r:?}"
                );
            }
        }
    }
}

#[test]
fn workspace_preserves_drafts_across_selection_and_commits_or_resets_explicitly() {
    let mut s = Source::new(Gallery::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    let save = named(&s, SemanticRole::Button, "Save changes").id;
    assert!(!s.handle_semantic_action(SemanticAction::Click { target: save }));
    click(
        &mut s,
        &mut tcx,
        SemanticRole::Switch,
        "Project notifications",
    );
    assert!(!s.view.workspace.projects[0].draft.notifications);
    click(&mut s, &mut tcx, SemanticRole::Button, "Docs");
    assert_eq!(s.view.workspace.selected, 2);
    assert!(s.view.workspace.projects[1].draft.notifications);
    click(&mut s, &mut tcx, SemanticRole::Button, "Plico");
    assert!(!s.view.workspace.projects[0].draft.notifications);
    click(&mut s, &mut tcx, SemanticRole::Button, "Save changes");
    assert!(!s.view.workspace.projects[0].saved.notifications);
    click(
        &mut s,
        &mut tcx,
        SemanticRole::CheckBox,
        "Save automatically",
    );
    click(&mut s, &mut tcx, SemanticRole::Button, "Reset changes");
    assert!(!s.view.workspace.projects[0].draft.autosave);
    let restored: Gallery = serde_json::from_value(serde_json::to_value(&s.view).unwrap()).unwrap();
    assert_eq!(restored, s.view);
}
#[test]
fn workspace_archive_empty_state_and_restore_keep_project_data() {
    let mut s = Source::new(Gallery::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    for _ in 0..3 {
        click(&mut s, &mut tcx, SemanticRole::Button, "Archive project");
    }
    assert_eq!(s.view.workspace.selected, 0);
    assert!(s.view.workspace.projects.iter().all(|p| p.archived));
    named(&s, SemanticRole::Label, "No active projects");
    click(&mut s, &mut tcx, SemanticRole::Button, "Restore projects");
    assert_eq!(s.view.workspace.selected, 1);
    assert!(s.view.workspace.projects.iter().all(|p| !p.archived));
}
#[test]
fn theme_density_changes_shared_widget_geometry_and_stabilizes() {
    let mut s = Source::new(Gallery::default());
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    let before = named(&s, SemanticRole::Button, "Save changes").bounds;
    click(&mut s, &mut tcx, SemanticRole::CheckBox, "Compact");
    assert_eq!(s.engine.theme().metrics, plico_core::Metrics::compact());
    let after = named(&s, SemanticRole::Button, "Save changes").bounds;
    assert!(after[3] - after[1] < before[3] - before[1]);
    frame(&mut s, &mut tcx, 920);
    assert_eq!(s.adapter.stats.layout_runs, 0);
}

#[test]
fn release_choices_and_disclosure_work_with_pointer_keyboard_and_semantics() {
    let mut s = Source::new(Gallery {
        router: plico_core::Router::new(Page::Controls),
        ..Default::default()
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 920);
    click(&mut s, &mut tcx, SemanticRole::Button, "Beta");
    assert_eq!(s.view.release_channel, 1);
    assert_eq!(
        named(&s, SemanticRole::Button, "Beta").states.selected,
        Some(true)
    );
    let details = named(&s, SemanticRole::Button, "Channel details");
    assert_eq!(details.states.expanded, Some(false));
    assert!(s.handle_semantic_action(SemanticAction::Focus { target: details.id }));
    s.handle_event(key("Enter", false));
    s.prepare_for_event(&mut tcx);
    frame(&mut s, &mut tcx, 920);
    assert!(s.view.release_details);
    assert_eq!(
        named(&s, SemanticRole::Button, "Channel details")
            .states
            .expanded,
        Some(true)
    );
    named(&s, SemanticRole::Label, "75%");
    click(&mut s, &mut tcx, SemanticRole::Button, "Nightly");
    named(&s, SemanticRole::Label, "40%");
    click(&mut s, &mut tcx, SemanticRole::Button, "Channel details");
    assert!(!s.view.release_details);
}
