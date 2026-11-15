use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};

#[test]
fn cui_showcase_exposes_dense_interactive_workspace() {
    let mut ui = Driver::new(
        Gallery {
            router: Router::new(Page::CuiShowcase),
            ..Default::default()
        },
        1600,
        1000,
    )
    .unwrap();

    ui.execute(Command::Click {
        role: "button".into(),
        label: "Show registration token".into(),
        occurrence: None,
    })
    .unwrap();
    ui.execute(Command::Click {
        role: "button".into(),
        label: "14:00".into(),
        occurrence: None,
    })
    .unwrap();
    ui.execute(Command::SetValue {
        label: "Fail below".into(),
        value: 75.,
    })
    .unwrap();

    assert!(ui.view().cui_showcase.token_visible);
    assert_eq!(ui.view().cui_showcase.selected_slot, 2);
    assert_eq!(ui.view().cui_showcase.coverage_floor, 75.);
    fn selected_calendar_cell(nodes: &[plico::ViewNode]) -> Option<plico::Style> {
        nodes.iter().find_map(|node| {
            (node.style.selected == Some(true) && node.text == "17")
                .then_some(node.style)
                .or_else(|| selected_calendar_cell(&node.children))
        })
    }
    let day = selected_calendar_cell(&ui.declarations()).expect("selected calendar day");
    assert_eq!((day.width, day.height), (Some(28.), Some(28.)));
    assert_eq!(day.radius, 14.);
    assert_eq!(day.corner_smoothing, 0.);
    assert_eq!(day.align, Some(plico::Align::Center));
    assert_eq!(day.justify, Some(plico::Justify::Center));
    assert_eq!(day.layout.align_self, Some(plico::Align::Center));
    assert_eq!(day.layout.justify_self, Some(plico::Align::Center));
    fn contains(nodes: &[plico::ViewNode], label: &str) -> bool {
        nodes
            .iter()
            .any(|node| node.text == label || contains(&node.children, label))
    }
    for label in [
        "Add a runner",
        "November 2026",
        "Reviewers",
        "Build minutes",
    ] {
        assert!(contains(&ui.declarations(), label), "missing {label}");
    }
}
