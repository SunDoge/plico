//! Sidebar animation behavior on a virtual clock: tween continuity, rapid
//! reversal, viewport auto-collapse, reduced motion and layout coexistence.
#![cfg(any(feature = "vello", feature = "skia"))]
use backend_settings::{Page, Settings};
use plico::VirtualClock;
use plico_renderer::{FrameSource, SemanticAction, SemanticRole, TextContext};
use plico_runtime::Source;
use std::time::Duration;

fn click(source: &mut Source<Settings>, label: &str) {
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, label, None)
        .unwrap_or_else(|| panic!("missing button {label}"))
        .id;
    assert!(source.handle_semantic_action(SemanticAction::Click { target: id }));
}

fn frame(source: &mut Source<Settings>, tcx: &mut TextContext, width: u32) {
    let _ = source.build_frame(tcx, width, 820);
}

fn sidebar_opacity(source: &Source<Settings>, label: &str) -> f32 {
    source
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("missing placed text {label}"))
        .paint
        .opacity
}

#[test]
fn sidebar_tween_hits_exact_midpoint_and_reverses_continuously() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 200.);

    // Collapse: 200 -> 72 over 260ms, EaseOut. The click settles in a frame
    // at t=0 (handle_semantic_action only retargets during the next
    // declaration), then the halfway sample at 130ms is exactly 88 with the
    // expanded furniture faded to its 0.125 share of the width span.
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100);
    clock.set(Duration::from_millis(130)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert!((source.view.sidebar_width() - 88.).abs() < 0.01);
    assert!((sidebar_opacity(&source, "plico") - 0.125).abs() < 0.001);
    assert!(source.view.animation_active());

    // Rapid reversal: the new flight continues from the value visible at the
    // click instant (88), not from 200 or 72.
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100); // settle the reversal at t=130
    clock.set(Duration::from_millis(140)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    // 10ms into 88 -> 200 with EaseOut: 88 + 112 * (1 - (1 - 10/260)^3).
    let expected = 88. + 112. * (1. - (1. - 10. / 260_f64).powi(3));
    assert!(
        (source.view.sidebar_width() as f64 - expected).abs() < 0.15,
        "reversal must continue from the visible value: {} vs {expected}",
        source.view.sidebar_width()
    );

    // Finite: the exact target lands and the app stops requesting frames.
    clock.set(Duration::from_millis(500)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 200.);
    assert!((sidebar_opacity(&source, "plico") - 1.).abs() < 0.001);
    assert!(
        !source.view.animation_active(),
        "an idle app must not keep the frame loop spinning"
    );
    // Idempotent rest: re-declaring at the same instant changes nothing.
    let width = source.view.sidebar_width();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), width);
}

#[test]
fn reduced_motion_snaps_mid_flight_and_completes_immediately() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();
    frame(&mut source, &mut tcx, 1100);
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100); // settle the click at t=0
    clock.set(Duration::from_millis(100)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert!(source.view.sidebar_width() > 72.);

    // Enabling reduced motion mid-flight lands on the target in the same frame.
    source.view.reduced_motion = true;
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 72.);
    assert!(!source.view.animation_active());
    assert_eq!(sidebar_opacity(&source, "plico"), 0.);

    // Later retargets complete immediately while the preference is on.
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 200.);
    assert!(!source.view.animation_active());
}

#[test]
fn narrow_viewport_auto_collapses_and_restores_the_user_preference() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 200.);

    // Below the breakpoint the sidebar collapses even though the user kept it
    // expanded; the preference field itself stays untouched.
    clock.set(Duration::from_millis(400)).unwrap();
    frame(&mut source, &mut tcx, 700); // settle the retarget at t=400
    clock.set(Duration::from_millis(800)).unwrap();
    frame(&mut source, &mut tcx, 700);
    assert_eq!(source.view.sidebar_width(), 72.);
    assert!(
        !source.view.compact,
        "auto-collapse must not edit the preference"
    );

    // Widening again restores the expanded preference without any click.
    clock.set(Duration::from_millis(800)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    clock.set(Duration::from_millis(1200)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 200.);
    assert!(!source.view.compact);

    // A user collapse sticks when the window narrows and widens again.
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100); // settle the click at t=1200
    clock.set(Duration::from_millis(1600)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 72.);
    frame(&mut source, &mut tcx, 700);
    assert_eq!(source.view.sidebar_width(), 72.);
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 72.);
    assert!(source.view.compact);
}

#[test]
fn icon_only_navigation_stays_accessible_and_layout_coexists_at_every_width() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();

    // Structural probe: walk the placed tree. A nav entry declares
    // button -> row -> [icon svg, label text]; the icon is found as the svg
    // whose row sibling is the route label, and the row parent resolves to
    // the button - no depth or coordinate filters anywhere.
    let probe = |source: &Source<Settings>, sidebar: f32, phase: &str| {
        let snapshot = source.adapter.semantics(&source.engine, 0);
        for label in ["Overview", "Activity", "Preferences"] {
            // Accessible name preserved at every width.
            assert!(
                snapshot
                    .node_by_role(SemanticRole::Button, label, None)
                    .is_some(),
                "nav entry {label} lost its accessible name ({phase})"
            );
            let button = snapshot
                .node_by_role(SemanticRole::Button, label, None)
                .unwrap();
            assert!(
                button.bounds[2] <= sidebar + 0.5,
                "{label} button overflows the sidebar at {phase}: {:?}",
                button.bounds
            );
            // The icon: the svg whose row sibling is the route label.
            // The nav label: a route-labeled text whose parent row also
            // parents an svg - the icon sibling - which the content-column
            // heading of the same name does not have.
            let row = source
                .adapter
                .placed
                .iter()
                .find(|p| {
                    p.paint.text.as_deref() == Some(label)
                        && p.parent_node_id.is_some_and(|row| {
                            source.adapter.placed.iter().any(|candidate| {
                                candidate.paint.svg.is_some()
                                    && candidate.parent_node_id == Some(row)
                            })
                        })
                })
                .map(|p| p.parent_node_id.expect("label has a row parent"))
                .unwrap_or_else(|| panic!("nav label {label} not placed ({phase})"));
            let icon = source
                .adapter
                .placed
                .iter()
                .find(|p| p.paint.svg.is_some() && p.parent_node_id == Some(row))
                .unwrap_or_else(|| panic!("nav icon missing under {label} ({phase})"));
            assert_eq!(
                (icon.rect[2] - icon.rect[0], icon.rect[3] - icon.rect[1]),
                (16., 16.),
                "{label} icon resized at {phase}: rect {:?}",
                icon.rect
            );
            assert!(
                icon.clip
                    .is_none_or(|c| c[0] <= icon.rect[0] && c[2] >= icon.rect[2]),
                "{label} icon clipped at {phase}: rect {:?} clip {:?}",
                icon.rect,
                icon.clip
            );
        }

        // The logo stays 26x26, separate from the 16px nav icons: it is
        // the svg that shares a row with the "plico" title text.
        let title_row = source
            .adapter
            .placed
            .iter()
            .find(|p| p.paint.text.as_deref() == Some("plico"))
            .map(|p| p.parent_node_id.expect("title has a row parent"))
            .expect("sidebar title placed");
        let logo = source
            .adapter
            .placed
            .iter()
            .find(|p| p.paint.svg.is_some() && p.parent_node_id == Some(title_row))
            .unwrap_or_else(|| panic!("logo missing ({phase})"));
        assert_eq!(logo.rect[2] - logo.rect[0], 26., "logo size ({phase})");

        // Content column starts after the sidebar; Details stays reachable.
        let details = snapshot
            .node_by_role(SemanticRole::Button, "Details", None)
            .unwrap_or_else(|| panic!("Details missing at {phase}"));
        assert!(
            details.bounds[0] >= sidebar - 0.5,
            "content overlaps the sidebar at {phase} width {sidebar}: {:?}",
            details.bounds
        );

        // The footer must not wrap into extra lines while it fades: it stays
        // single-line (ellipsis policy), so the toggle above it never jumps.
        let footer = source
            .adapter
            .placed
            .iter()
            .find(|p| p.paint.text.as_deref() == Some("Backend playground"))
            .unwrap_or_else(|| panic!("footer missing ({phase})"));
        assert!(
            footer.rect[3] - footer.rect[1] <= 20.,
            "footer wrapped to extra lines at {phase}: {:?}",
            footer.rect
        );
        let toggle = snapshot
            .node_by_role(SemanticRole::Button, "Toggle sidebar", None)
            .unwrap();
        let _ = toggle;
    };

    // Expanded.
    frame(&mut source, &mut tcx, 1100);
    probe(&source, 200., "expanded");

    // Intermediate: mid-flight width with a half-faded label.
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100); // settle the click at t=0
    clock.set(Duration::from_millis(130)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    probe(&source, source.view.sidebar_width(), "intermediate");

    // Compact: label at opacity 0 still occupies flex space, but the icon
    // must neither shrink nor be pushed out of the clipped column.
    clock.set(Duration::from_millis(500)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    probe(&source, 72., "compact");

    // The icon-only entry still navigates: clicking Activity by its
    // accessible name routes the app without any explicit request_rebuild.
    click(&mut source, "Activity");
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.router.current(), Page::Activity);
}

#[test]
fn host_scheduling_goes_idle_at_rest_and_animating_while_in_flight() {
    // Assert through FrameSource itself - the same contract the desktop host
    // uses to choose between the redraw loop, a deadline wake or full idle.
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();

    use plico_renderer::FrameSource as _;
    assert!(
        !source.has_anim(),
        "an idle app must not request the redraw loop"
    );
    assert_eq!(
        source.animation_deadline(),
        None,
        "an idle app has no deferred repaint deadline"
    );

    frame(&mut source, &mut tcx, 1100);
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100); // settle the click at t=0
    assert!(source.has_anim(), "in-flight tween drives the redraw loop");

    clock.set(Duration::from_millis(260)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert!(
        !source.has_anim(),
        "the finished tween stops the redraw loop"
    );
}

#[test]
fn sidebar_motion_coexists_with_the_open_inspector() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();

    // Open the inspector (its own 0 -> 248 tween), then collapse the sidebar
    // mid-flight: both tweens run together and the columns never overlap.
    frame(&mut source, &mut tcx, 1100);
    click(&mut source, "Details");
    frame(&mut source, &mut tcx, 1100); // settle the inspector at t=0
    clock.set(Duration::from_millis(130)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    // EaseOut at half the duration: 248 * (1 - (1 - 0.5)^3) = 217.
    assert!(
        (source.view.inspector_width() - 217.).abs() < 0.01,
        "inspector halfway: {}",
        source.view.inspector_width()
    );

    // Collapse the sidebar while the inspector is still animating.
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100); // settle the sidebar collapse at t=130
    clock.set(Duration::from_millis(260)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    let sidebar = source.view.sidebar_width();
    // The sidebar collapse is 130ms into its own 260ms flight: EaseOut from
    // 200 gives exactly 88 at this instant, mid-flight alongside the pane.
    assert!(
        (sidebar - 88.).abs() < 0.01,
        "sidebar mid-flight: {sidebar}"
    );
    // The content column ends where the inspector pane begins; with the
    // sidebar at any width the two panes stay disjoint.
    let semantics = source.adapter.semantics(&source.engine, 0);
    let details = semantics
        .node_by_role(SemanticRole::Button, "Details", None)
        .expect("Details entry point");
    assert!(
        details.bounds[0] >= sidebar - 0.5,
        "content overlaps the sidebar while the inspector animates: {:?}",
        details.bounds
    );

    // Both tweens finish together and the app returns to idle.
    clock.set(Duration::from_millis(600)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 72.);
    assert_eq!(source.view.inspector_width(), 248.);
    use plico_renderer::FrameSource as _;
    assert!(!source.has_anim());
}

#[test]
fn narrow_mode_activation_cannot_mutate_the_saved_preference() {
    let clock = VirtualClock::default();
    let mut source = Source::with_clock(Settings::default(), clock.clone());
    let mut tcx = TextContext::new();

    // Widened: the toggle is enabled and flips the preference.
    frame(&mut source, &mut tcx, 1100);
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100);
    assert!(source.view.compact);

    // Back to expanded for a clean state.
    click(&mut source, "Toggle sidebar");
    frame(&mut source, &mut tcx, 1100);
    assert!(!source.view.compact);

    // Narrow: the viewport forces the collapse and the toggle is disabled.
    // A semantic activation is refused, so the saved preference stays
    // expanded - a later wide window restores it.
    frame(&mut source, &mut tcx, 700); // settle the enforced collapse at t=0
    clock.set(Duration::from_millis(300)).unwrap();
    frame(&mut source, &mut tcx, 700);
    let id = source
        .adapter
        .semantics(&source.engine, 0)
        .node_by_role(SemanticRole::Button, "Toggle sidebar", None)
        .unwrap()
        .id;
    assert!(
        !source.handle_semantic_action(SemanticAction::Click { target: id }),
        "the disabled toggle must refuse activation while narrow"
    );
    assert!(!source.view.compact, "the saved preference is untouched");
    assert_eq!(
        source.view.sidebar_width(),
        72.,
        "the viewport-enforced collapse still applies"
    );

    // Widening again restores the saved expanded state without any click.
    clock.set(Duration::from_millis(600)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    clock.set(Duration::from_millis(1000)).unwrap();
    frame(&mut source, &mut tcx, 1100);
    assert_eq!(source.view.sidebar_width(), 200.);
    assert!(!source.view.compact);
}
