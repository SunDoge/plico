use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
#[test]
fn glass_profile_and_clarity_controls_update_the_gallery_without_enabling_motion() {
    let mut app = Gallery {
        router: Router::new(Page::Effects),
        ..Default::default()
    };
    app.effects.paused = true;
    let mut driver = Driver::new(app, 1120, 900).unwrap();
    for (label, value) in [
        ("Regular", "regular"),
        ("Clear", "clear"),
        ("Custom", "custom"),
    ] {
        driver
            .execute(Command::Click {
                role: "button".into(),
                label: label.into(),
                occurrence: None,
            })
            .unwrap();
        assert_eq!(driver.view().effects.glass_style, value);
        assert!(driver.view().effects.paused);
    }
    driver
        .execute(Command::SetValue {
            label: "Pane clarity".into(),
            value: 0.8,
        })
        .unwrap();
    assert!((driver.view().effects.clarity - 0.8).abs() < 1e-6);
    driver
        .execute(Command::Click {
            role: "button".into(),
            label: "Unavailable".into(),
            occurrence: None,
        })
        .unwrap_err();
}

#[test]
fn preview_pointer_commands_capture_hover_press_and_release_without_changing_button_geometry() {
    use std::time::Duration;
    let mut app = Gallery {
        router: Router::new(Page::Effects),
        ..Default::default()
    };
    app.effects.paused = true;
    app.effects.glass_style = "regular".into();
    let mut driver = Driver::new(app, 1120, 900).unwrap();
    let bounds = driver
        .snapshot()
        .exposed_nodes()
        .iter()
        .find(|n| n.label.as_deref() == Some("Continue"))
        .unwrap()
        .bounds;
    let x = f64::from((bounds[0] + bounds[2]) * 0.5);
    let y = f64::from((bounds[1] + bounds[3]) * 0.5);
    driver.execute(Command::PointerMove { x, y }).unwrap();
    driver.advance(Duration::from_millis(120)).unwrap();
    driver.execute(Command::PointerDown { x, y }).unwrap();
    driver.advance(Duration::from_millis(150)).unwrap();
    assert!(driver.placed_nodes().iter().filter_map(|n|n.paint.widget.as_ref()).flat_map(|s|s.commands.iter()).any(|c|matches!(c,plico_renderer::PaintCommand::Shader {effect,..} if effect.values().len()>=20 && effect.values()[17]==1.)));
    let after = driver
        .snapshot()
        .exposed_nodes()
        .iter()
        .find(|n| n.label.as_deref() == Some("Continue"))
        .unwrap()
        .bounds;
    assert_eq!(bounds, after);
    driver.execute(Command::PointerUp { x, y }).unwrap();
    driver.advance(Duration::from_millis(150)).unwrap();
}
