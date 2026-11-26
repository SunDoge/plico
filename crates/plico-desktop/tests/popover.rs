use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    TextContext, UiEvent,
};
use std::time::Duration;
#[derive(Default)]
struct Demo {
    popup: PopoverState,
    clicks: usize,
    disabled: bool,
    shows: usize,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        column(cx)
            .width_percent(100.)
            .height_percent(100.)
            .padding(12)
            .justify(Justify::End)
            .align(Align::End)
            .children(|cx| {
                popover(cx, &mut self.popup, "Options")
                    .width(240.)
                    .disabled(self.disabled)
                    .motion(Motion::Quick)
                    .children(|cx| {
                        text(cx, "Project options");
                        if button(cx).text("Apply").clicked() {
                            self.clicks += 1;
                            cx.close_popover();
                        }
                        button(cx).text("Unavailable").disabled(true);
                    });
                button(cx).text("After");
            })
    }
}
fn action(s: &mut Source<Demo>, label: &str, focus: bool) {
    let id = s
        .semantics()
        .node_by_role(SemanticRole::Button, label, None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(if focus {
        SemanticAction::Focus { target: id }
    } else {
        SemanticAction::Click { target: id }
    }));
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
#[test]
fn anchored_panel_flips_clamps_and_routes_action_in_placed_geometry() {
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 320, 240);
    action(&mut s, "Options", true);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 320, 240);
    let snapshot = s.semantics();
    let trigger = snapshot
        .node_by_role(SemanticRole::Button, "Options", None)
        .unwrap();
    let panel = snapshot
        .node_by_role(SemanticRole::Dialog, "Options", None)
        .unwrap();
    assert_eq!(panel.states.modal, Some(false));
    assert_eq!(snapshot.modal_root, None);
    assert!(panel.bounds[3] <= trigger.bounds[1] - 5.);
    assert!(panel.bounds[0] >= 8. && panel.bounds[2] <= 312.);
    let apply = snapshot
        .node_by_role(SemanticRole::Button, "Apply", None)
        .unwrap();
    assert_eq!(snapshot.focus, Some(apply.id));
    assert!(apply.bounds[1] >= panel.bounds[1] && apply.bounds[3] <= panel.bounds[3]);
    action(&mut s, "Apply", false);
    s.build_frame(&mut tcx, 320, 240);
    assert_eq!(s.view().clicks, 1);
    assert!(!s.view().popup.is_open());
    assert_eq!(s.semantics().focus, Some(trigger.id));
}
#[test]
fn tab_leaves_popup_and_exit_does_not_steal_focus() {
    let clock = plico::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    action(&mut s, "Options", true);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 600, 400);
    assert!(s.handle_event(key("Tab")).handled);
    s.build_frame(&mut tcx, 600, 400);
    assert!(!s.view().popup.is_open());
    let snapshot = s.semantics();
    let after = snapshot
        .node_by_role(SemanticRole::Button, "After", None)
        .unwrap()
        .id;
    assert_eq!(snapshot.focus, Some(after));
    clock.advance(Duration::from_secs(1)).unwrap();
    s.build_frame(&mut tcx, 600, 400);
    assert_eq!(s.semantics().focus, Some(after));
}
#[test]
fn disabled_trigger_does_not_open_and_popup_state_is_portable() {
    let state: PopoverState = serde_json::from_str("{\"open\":true}").unwrap();
    let mut s = Source::new(Demo {
        popup: state,
        disabled: true,
        ..Default::default()
    });
    s.build_frame(&mut TextContext::new(), 180, 100);
    assert!(!s.view().popup.is_open());
    assert!(
        s.semantics()
            .node_by_role(SemanticRole::Dialog, "Options", None)
            .is_none()
    );
}

#[test]
fn pointer_uses_positioned_panel_and_resize_reanchors_without_ui_state_reset() {
    use plico_renderer::{Point, PointerButton, PointerEvent, PointerPhase};
    let mut s = Source::new(Demo::default());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 600, 400);
    s.build_frame(&mut tcx, 260, 220);
    let snapshot = s.semantics();
    let panel = snapshot
        .node_by_role(SemanticRole::Dialog, "Options", None)
        .unwrap();
    assert!(panel.bounds[0] >= 8. && panel.bounds[2] <= 252.);
    let item = snapshot
        .node_by_role(SemanticRole::Button, "Apply", None)
        .unwrap();
    let x = (item.bounds[0] + item.bounds[2]) as f64 / 2.;
    let y = (item.bounds[1] + item.bounds[3]) as f64 / 2.;
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        s.handle_event(UiEvent::Pointer(PointerEvent {
            phase,
            position: Point { x, y },
            button: Some(PointerButton::Primary),
            buttons: 1,
            modifiers: Modifiers::empty(),
            properties: Default::default(),
        }));
    }
    s.build_frame(&mut tcx, 260, 220);
    assert_eq!(s.view().clicks, 1);
    assert!(!s.view().popup.is_open());
}

#[derive(Default)]
struct Nested {
    dialog: ModalState,
    popup: PopoverState,
}
impl HostView for Nested {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        button(cx).text("Background");
        modal(cx, &mut self.dialog)
            .title("Parent")
            .motion(Motion::None)
            .children(|cx| {
                popover(cx, &mut self.popup, "Child popup")
                    .motion(Motion::None)
                    .children(|cx| {
                        button(cx).text("Child action");
                    });
                button(cx).text("Parent action");
            });
    }
}
#[test]
fn popup_inside_modal_is_exposed_to_accessibility_and_escape_restores_parent_scope() {
    let mut view = Nested::default();
    view.dialog.open();
    let mut s = Source::new(view);
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    let trigger = s
        .semantics()
        .node_by_role(SemanticRole::Button, "Child popup", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(SemanticAction::Click { target: trigger }));
    s.build_frame(&mut tcx, 600, 400);
    let snapshot = s.semantics();
    assert!(
        snapshot
            .node_by_role(SemanticRole::Button, "Child action", None)
            .is_some()
    );
    assert!(
        snapshot
            .node_by_role(SemanticRole::Button, "Background", None)
            .is_none()
    );
    let parent_action = snapshot
        .node_by_role(SemanticRole::Button, "Parent action", None)
        .unwrap()
        .id;
    assert!(!s.handle_semantic_action(SemanticAction::Click {
        target: parent_action
    }));
    assert_eq!(
        snapshot
            .node_by_role(SemanticRole::Button, "Child popup", None)
            .unwrap()
            .controls
            .len(),
        1
    );
    s.handle_event(key("Escape"));
    s.build_frame(&mut tcx, 600, 400);
    assert!(s.view().dialog.is_open());
    assert!(!s.view().popup.is_open());
    assert_eq!(s.semantics().focus, Some(trigger));
    assert!(s.handle_semantic_action(SemanticAction::Click {
        target: parent_action
    }));
}

#[test]
fn popup_animation_frames_keep_application_and_layout_idle() {
    let clock = plico::VirtualClock::default();
    let mut s = Source::with_clock(Demo::default(), clock.clone());
    let mut tcx = TextContext::new();
    s.build_frame(&mut tcx, 600, 400);
    action(&mut s, "Options", false);
    s.build_frame(&mut tcx, 600, 400);
    let before = s.view().shows;
    clock.advance(Duration::from_millis(70)).unwrap();
    s.build_frame(&mut tcx, 600, 400);
    assert_eq!(s.view().shows, before);
    assert_eq!(s.frame_diagnostics().backend.layout_runs, 0);
}

#[test]
fn automatic_popup_focus_preserves_pointer_or_keyboard_modality() {
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(Demo::default(), clock.clone());
    let mut text = TextContext::new();
    source.build_frame(&mut text, 400, 400);
    action(&mut source, "Options", false);
    source.build_frame(&mut text, 400, 400);
    clock.advance(Duration::from_secs(1)).unwrap();
    let nodes = source.build_frame(&mut text, 400, 400);
    assert!(source.view().popup.is_open());
    assert!(
        nodes.iter().all(|n| n.paint.outline_width == 0.),
        "automatic popup focus must not opt pointer users into focus rings"
    );
    action(&mut source, "Apply", false);
    source.build_frame(&mut text, 400, 400);
    clock.advance(Duration::from_secs(1)).unwrap();
    let nodes = source.build_frame(&mut text, 400, 400);
    assert!(
        nodes.iter().all(|n| n.paint.outline_width == 0.),
        "restoring focus must preserve pointer modality too"
    );
    action(&mut source, "Options", true);
    action(&mut source, "Options", false);
    source.build_frame(&mut text, 400, 400);
    clock.advance(Duration::from_secs(1)).unwrap();
    let nodes = source.build_frame(&mut text, 400, 400);
    assert!(
        nodes.iter().any(|n| n.paint.outline_width > 0.),
        "keyboard focus must remain visible inside the popup"
    );
}

#[test]
fn glass_popup_pose_frames_reuse_layout_and_move_the_shader_canvas() {
    use plico::widgets::liquid;
    use plico_renderer::{PaintCommand, Scene};

    struct GlassPopup {
        state: PopoverState,
        selected: i32,
        centered: bool,
        shows: usize,
    }
    impl HostView for GlassPopup {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            self.shows += 1;
            column(cx).padding(24).children(|cx| {
                for label in ["One", "Two", "Three", "Four"] {
                    cx.keyed(label, |cx| {
                        text(cx, label);
                    });
                }
                if self.centered {
                    liquid::dropdown_menu(cx, &mut self.state, "Options")
                        .over_anchor(true)
                        .children(|cx| {
                            menu_item(cx, "Archive").clicked();
                            menu_item(cx, "Report").clicked();
                            menu_item(cx, "Snooze").clicked();
                        });
                } else {
                    liquid::select(
                        cx,
                        &mut self.state,
                        "Delivery",
                        &mut self.selected,
                        &[
                            SelectOption::new("regular", 0, "Regular"),
                            SelectOption::new("express", 1, "Express"),
                            SelectOption::new("overnight", 2, "Overnight"),
                        ],
                    )
                    .response();
                }
            });
        }
    }
    for centered in [true, false] {
        let clock = plico::VirtualClock::default();
        let mut source = Source::with_clock(
            GlassPopup {
                state: PopoverState::default(),
                selected: 0,
                centered,
                shows: 0,
            },
            clock.clone(),
        );
        let mut text = TextContext::new();
        let mut scene = Scene::new();
        let render =
            |source: &mut Source<GlassPopup>, scene: &mut Scene, text: &mut TextContext| {
                source.render_frame(scene, text, (640, 480), 1.).unwrap();
            };
        render(&mut source, &mut scene, &mut text);
        source.update(|view| view.state.open());
        render(&mut source, &mut scene, &mut text);
        let shows = source.view().shows;

        clock.advance(Duration::from_millis(64)).unwrap();
        render(&mut source, &mut scene, &mut text);
        assert_eq!(source.frame_diagnostics().backend.layout_runs, 0);
        // Cross the trigger-to-surface handoff without rebuilding the view.
        clock.advance(Duration::from_millis(110)).unwrap();
        render(&mut source, &mut scene, &mut text);
        assert_eq!(source.view().shows, shows);

        let mut previous = None;
        for _ in 0..6 {
            clock.advance(Duration::from_millis(16)).unwrap();
            render(&mut source, &mut scene, &mut text);
            let nodes = source.placed_nodes();
            let (layout, canvas) = nodes
                .iter()
                .find_map(|node| {
                    node.paint
                        .widget
                        .as_ref()?
                        .commands
                        .iter()
                        .find_map(|command| {
                            if let PaintCommand::Shader { effect, .. } = command
                                && effect.values().get(13) == Some(&5.)
                            {
                                return Some((node.rect, effect.logical_size()));
                            }
                            None
                        })
                })
                .expect("animated glass panel");
            if let Some((prior_layout, prior_canvas)) = previous {
                assert_eq!(layout, prior_layout);
                assert_ne!(canvas, prior_canvas);
            }
            previous = Some((layout, canvas));
            let stats = source.frame_diagnostics().backend;
            assert_eq!(stats.layout_runs, 0, "centered={centered}: {stats:?}");
            assert_eq!(stats.flatten_visits, 0, "centered={centered}: {stats:?}");
            assert_eq!(stats.text_measure_invalidations, 0);
            assert_eq!(source.view().shows, shows);
            assert_eq!(stats.sync_visits, 0);
            assert!(stats.scene_parts_reused > 0, "{stats:?}");
            assert!(
                stats.scene_parts_rebuilt > 0 && stats.scene_parts_rebuilt < nodes.len(),
                "{stats:?}"
            );
        }
    }
}

#[cfg(feature = "wgpu")]
#[test]
#[ignore = "requires a working wgpu adapter and system fonts"]
fn retained_popup_fragments_match_full_encoding_through_reversal() {
    use plico::{VirtualClock, widgets::liquid};
    use plico_renderer::scene::build_presented_scene_scaled_layers;
    use plico_renderer::{PaintCommand, Scene};
    use plico_wgpu::Renderer;

    struct Menu {
        state: PopoverState,
    }

    impl plico_desktop::HostView for Menu {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width_percent(100.)
                .height_percent(100.)
                .clip(true)
                .align(Align::Center)
                .justify(Justify::Center)
                .children(|cx| {
                    row(cx).absolute(20., 55.).gap(8).children(|cx| {
                        for (index, color) in [0xfac2b4, 0xf5e8aa, 0xbcddef].into_iter().enumerate()
                        {
                            cx.keyed(index as u64, |cx| {
                                column(cx)
                                    .width(96.)
                                    .height(135.)
                                    .radius(12.)
                                    .background(Color::Literal(color))
                                    .children(|cx| {
                                        text(cx, "Backdrop");
                                    });
                            });
                        }
                    });
                    liquid::dropdown_menu(cx, &mut self.state, "Options")
                        .over_anchor(true)
                        .children(|cx| {
                            menu_item(cx, "Rename").clicked();
                            menu_item(cx, "Duplicate").clicked();
                            menu_item(cx, "Share").clicked();
                        });
                });
        }
    }

    let clock = VirtualClock::default();
    let mut source = Source::with_clock(
        Menu {
            state: PopoverState::default(),
        },
        clock.clone(),
    )
    .with_background(peniko::Color::WHITE);
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, 1.).unwrap();
    let mut retained = Scene::new();
    let mut full = Scene::new();
    let size = (360, 260);
    let mut reused = 0;
    let mut checked_scaled_text = false;
    for (delta, open) in [
        (0, Some(true)),
        (80, None),
        (44, None),
        (56, None),
        (40, Some(false)),
        (40, None),
        (45, Some(true)),
        (80, None),
        (750, None),
        (0, Some(false)),
        (800, None),
    ] {
        clock.advance(Duration::from_millis(delta)).unwrap();
        if let Some(open) = open {
            source.update(|view| {
                if open {
                    view.state.open();
                } else {
                    view.state.close();
                }
            });
        }
        source
            .render_frame(&mut retained, &mut text, size, 1.)
            .unwrap();
        reused += source.frame_diagnostics().backend.scene_parts_reused;
        for command in &retained.flattened().commands {
            if let PaintCommand::Paragraph { request, transform } = command
                && request.text.as_ref() == "Rename"
            {
                let scale = transform.as_coeffs()[0];
                if (scale - 1.).abs() > 0.02 {
                    assert_eq!(request.raster_scale_hint, Some(1.));
                    checked_scaled_text = true;
                }
            }
        }
        build_presented_scene_scaled_layers(
            &mut full,
            &[source.placed_nodes()],
            &mut text,
            size.0,
            size.1,
            peniko::Color::WHITE,
            1.,
        );
        let actual = renderer
            .render_scene(&retained, [size.0, size.1], [1.; 4], true)
            .unwrap();
        let expected = renderer
            .render_scene(&full, [size.0, size.1], [1.; 4], false)
            .unwrap();
        assert!(
            actual == expected,
            "retained pixels differ at {:?}",
            source.frame_diagnostics()
        );
    }
    assert!(reused > 0);
    assert!(checked_scaled_text);
    assert!(!source.view().state.is_open());
}
