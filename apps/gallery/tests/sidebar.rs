#![cfg(feature = "host")]
use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
use plico_renderer::SemanticRole;
use std::time::Duration;

fn click(driver: &mut Driver<Gallery>, label: &str) {
    driver
        .execute(Command::Click {
            role: "button".into(),
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
        .node_by_role(SemanticRole::Button, "Motion", None)
        .unwrap()
        .id;
    let expanded = heading_x(&driver);
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Button, "Pause shader", None)
            .unwrap()
            .bounds[2]
            <= 1120.
    );
    click(&mut driver, "Toggle navigation");
    driver.advance(Duration::from_millis(100)).unwrap();
    let intermediate = heading_x(&driver);
    driver.advance(Duration::from_millis(250)).unwrap();
    let collapsed = heading_x(&driver);
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Button, "Pause shader", None)
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
            .node_by_role(SemanticRole::Button, "Motion", None)
            .unwrap()
            .id,
        target
    );
    click(&mut driver, "Motion");
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Label, "Motion studies", None)
            .is_some()
    );
    click(&mut driver, "Toggle navigation");
    driver.advance(Duration::from_millis(400)).unwrap();
    assert_eq!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Button, "Motion", None)
            .unwrap()
            .id,
        target
    );
}
