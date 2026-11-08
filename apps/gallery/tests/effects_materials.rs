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
        assert_eq!(driver.source.view.effects.glass_style, value);
        assert!(driver.source.view.effects.paused);
    }
    driver
        .execute(Command::SetValue {
            label: "Pane clarity".into(),
            value: 0.8,
        })
        .unwrap();
    assert!((driver.source.view.effects.clarity - 0.8).abs() < 1e-6);
    driver
        .execute(Command::Click {
            role: "button".into(),
            label: "Unavailable".into(),
            occurrence: None,
        })
        .unwrap_err();
}
