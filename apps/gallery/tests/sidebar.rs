#![cfg(feature = "host")]
use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, DifferentialTester, Driver};
use plico_renderer::{SemanticRole, SemanticSnapshot};
use std::time::Duration;

fn click(driver: &mut Driver<Gallery>, role: &str, label: &str) {
    driver
        .execute(Command::Click {
            role: role.into(),
            label: label.into(),
            occurrence: None,
        })
        .unwrap();
}
fn heading_x(driver: &Driver<Gallery>) -> f32 {
    driver
        .snapshot()
        .node_by_role(SemanticRole::Label, "Workspace / Gallery", None)
        .unwrap()
        .bounds[0]
}
fn heading_edge(frame: &SemanticSnapshot) -> Result<f32, String> {
    frame
        .node_by_role(SemanticRole::Label, "Workspace / Gallery", None)
        .map(|node| node.bounds[0])
        .ok_or_else(|| "Workspace / Gallery heading disappeared".into())
}

#[test]
fn sidebar_moves_content_in_one_direction_per_toggle() {
    let mut ui = Driver::new(Gallery::default(), 1120, 820).unwrap();
    let expanded = heading_x(&ui);
    for (direction, label) in [(-1., "collapse"), (1., "expand")] {
        let start = heading_x(&ui);
        ui.click("Toggle navigation").unwrap();
        let mut moving_frames = 0;
        ui.check_frames(label, 30, Duration::from_millis(10), |before, after| {
            let previous = heading_edge(before)?;
            let current = heading_edge(after)?;
            let progress = direction * (current - previous);
            if !progress.is_finite() || progress < -0.01 {
                return Err(format!(
                    "heading left edge moved from {previous} to {current} DIP"
                ));
            }
            if progress > 0.01 {
                moving_frames += 1;
            }
            Ok(())
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let end = heading_x(&ui);
        assert!(
            (direction * (end - start) - 160.).abs() < 1.,
            "{label}: expected 160 DIP travel, got {start} to {end}"
        );
        assert!(moving_frames > 1, "{label}: jumped to the endpoint");
    }
    assert_eq!(heading_x(&ui), expanded);
}

#[test]
fn expanding_never_squeezes_the_header_title() {
    let mut driver = Driver::new(Gallery::default(), 1120, 820).unwrap();
    click(&mut driver, "button", "Toggle navigation");
    driver.advance(Duration::from_millis(400)).unwrap();
    click(&mut driver, "button", "Toggle navigation");
    for _ in 0..25 {
        driver.advance(Duration::from_millis(10)).unwrap();
        if let Some(node) = driver
            .snapshot()
            .node_by_role(SemanticRole::Label, "plico", None)
        {
            let width = node.bounds[2] - node.bounds[0];
            assert!(
                width >= 40.,
                "header title squeezed to {width} px during the expand animation"
            );
        }
    }
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Label, "plico", None)
            .is_some(),
        "header title should be visible once the sidebar is expanded"
    );
}

#[test]
fn collapsing_animates_layout_without_replacing_navigation_targets() {
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::Patterns),
            ..Default::default()
        },
        1120,
        820,
    )
    .unwrap();
    let target = driver
        .snapshot()
        .node_by_role(SemanticRole::TreeItem, "Motion", None)
        .unwrap()
        .id;
    let expanded = heading_x(&driver);
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Button, "Toast", None)
            .unwrap()
            .bounds[2]
            <= 1120.
    );
    click(&mut driver, "button", "Toggle navigation");
    driver.advance(Duration::from_millis(100)).unwrap();
    let intermediate = heading_x(&driver);
    driver.advance(Duration::from_millis(250)).unwrap();
    let collapsed = heading_x(&driver);
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Button, "Toast", None)
            .unwrap()
            .bounds[2]
            <= 1120.
    );
    let theme = driver
        .snapshot()
        .node_by_role(SemanticRole::Switch, "Dark theme", None)
        .unwrap()
        .bounds;
    assert!(
        theme[2] <= 1120.,
        "theme control escaped viewport: {theme:?}"
    );
    assert!(
        expanded > intermediate && intermediate > collapsed,
        "width should interpolate: {expanded}, {intermediate}, {collapsed}"
    );
    assert_eq!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::TreeItem, "Motion", None)
            .unwrap()
            .id,
        target
    );
    click(&mut driver, "treeitem", "Motion");
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Label, "Motion studies", None)
            .is_some()
    );
    click(&mut driver, "button", "Toggle navigation");
    driver.advance(Duration::from_millis(400)).unwrap();
    assert_eq!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::TreeItem, "Motion", None)
            .unwrap()
            .id,
        target
    );
}

#[test]
fn gallery_navigation_matches_a_cold_replay() {
    let mut ui = DifferentialTester::new(Gallery::default(), 1120, 820).unwrap();
    ui.execute(Command::Click {
        role: "treeitem".into(),
        label: "Controls".into(),
        occurrence: None,
    })
    .unwrap();
    ui.advance(Duration::from_millis(80)).unwrap();
    ui.resize(960, 720).unwrap();
    ui.settle().unwrap();

    assert_eq!(ui.view().router.current(), Page::Controls);
    assert!(
        ui.snapshot()
            .node_by_role(SemanticRole::Button, "Increment", None)
            .is_some()
    );
}
