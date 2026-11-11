use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
use std::time::Duration;
fn search() -> Driver<Gallery> {
    let mut app = Gallery {
        router: Router::new(Page::ShaderPlayground),
        ..Default::default()
    };
    app.shader_playground.study = "search".into();
    Driver::new(app, 1120, 820).unwrap()
}
fn click(driver: &mut Driver<Gallery>, label: &str) {
    driver
        .execute(Command::Click {
            role: "button".into(),
            label: label.into(),
            occurrence: None,
        })
        .unwrap();
}
fn has_input(driver: &Driver<Gallery>) -> bool {
    driver
        .snapshot()
        .exposed_nodes()
        .iter()
        .any(|n| n.label.as_deref() == Some("Search"))
}
#[test]
fn liquid_search_reveals_real_input_only_after_spring_settles_and_preserves_query() {
    let mut driver = search();
    assert!(!has_input(&driver));
    click(&mut driver, "Release drop");
    assert!(!has_input(&driver), "field cannot jump ahead of the glass");
    driver.advance(Duration::from_millis(160)).unwrap();
    assert!(!has_input(&driver));
    driver.advance(Duration::from_secs(3)).unwrap();
    assert!(has_input(&driver));
    driver
        .execute(Command::Focus {
            role: "textbox".into(),
            label: "Search".into(),
            occurrence: None,
        })
        .unwrap();
    driver
        .execute(Command::Text {
            text: "Plico".into(),
        })
        .unwrap();
    assert_eq!(driver.view().shader_playground.query, "Plico");
    click(&mut driver, "Attach drop");
    driver.advance(Duration::from_secs(3)).unwrap();
    assert!(!has_input(&driver));
    click(&mut driver, "Pause");
    click(&mut driver, "Release drop");
    assert!(has_input(&driver), "paused geometry snaps with its content");
    assert_eq!(driver.view().shader_playground.query, "Plico");
}
#[test]
fn top_edge_drag_updates_the_liquid_and_releases_to_a_capsule() {
    let mut driver = search();
    let bounds = driver
        .snapshot()
        .exposed_nodes()
        .iter()
        .find(|n| n.label.as_deref() == Some("Pull liquid drop"))
        .unwrap()
        .bounds;
    let x = f64::from((bounds[0] + bounds[2]) * 0.5);
    let y = f64::from(bounds[1] + 20.);
    driver.execute(Command::PointerDown { x, y }).unwrap();
    driver
        .execute(Command::PointerMove { x, y: y + 180. })
        .unwrap();
    assert!(
        driver.view().shader_playground.drop > 0.5,
        "captured drag continues beyond handle bounds"
    );
    driver
        .execute(Command::PointerUp { x, y: y + 180. })
        .unwrap();
    driver.advance(Duration::from_secs(3)).unwrap();
    assert_eq!(driver.view().shader_playground.drop, 1.);
    assert!(has_input(&driver));
}
