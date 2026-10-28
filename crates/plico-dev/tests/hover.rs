use plico::prelude::*;
use plico_dev::{App, Request, render};
use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
struct Demo {
    hovered: bool,
    clicks: u32,
    clicked_hovered: bool,
}
impl App for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let response = button(cx).key("action").text("Action").response();
        self.hovered = response.hovered();
        if response.clicked() {
            self.clicks += 1;
            self.clicked_hovered = response.hovered();
        }
        text!(cx, "Hovered: {}", response.hovered());
    }
}
#[test]
fn logical_hover_survives_module_restore_settling_and_click_consumption() {
    let first = render::<Demo>(Request::default()).unwrap();
    let button = first.nodes.iter().find(|n| n.text == "Action").unwrap();
    let path = vec![button.key.clone()];
    let request = Request {
        state: first.state,
        hovered: Some(path.clone()),
        clicks: vec![path],
        ..Default::default()
    };
    let wire = serde_json::to_string(&request).unwrap();
    let frame = render::<Demo>(serde_json::from_str(&wire).unwrap()).unwrap();
    let state = frame.state.decode::<Demo>().unwrap();
    assert!(state.hovered);
    assert!(state.clicked_hovered);
    assert_eq!(state.clicks, 1);
    let frame = render::<Demo>(Request {
        state: frame.state,
        hovered: None,
        ..Default::default()
    })
    .unwrap();
    let state = frame.state.decode::<Demo>().unwrap();
    assert!(!state.hovered);
    assert_eq!(state.clicks, 1);
}
