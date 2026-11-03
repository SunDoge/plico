use plico::UiContext;
use plico_dev::{App, Request, StateSnapshot, render};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Serialize, Deserialize)]
struct RichState {
    entries: BTreeMap<(u32, u32), String>,
    #[serde(with = "serde_bytes")]
    bytes: Vec<u8>,
    infinity: f64,
    nan: f64,
}
impl App for RichState {
    fn show(&mut self, _: &mut UiContext<'_>) {}
}

#[test]
fn named_fields_allow_reordering_additions_and_removal() {
    #[derive(Serialize)]
    struct Before {
        count: u64,
        label: String,
        removed: bool,
    }
    #[derive(Deserialize, PartialEq, Debug)]
    struct After {
        label: String,
        #[serde(default)]
        added: bool,
        count: u64,
    }
    let state = StateSnapshot::encode(&Before {
        count: 42,
        label: "retained".into(),
        removed: true,
    })
    .unwrap();
    let restored: After = state.decode().unwrap();
    assert_eq!(
        restored,
        After {
            label: "retained".into(),
            added: false,
            count: 42
        }
    );
}

#[test]
fn corrupt_snapshot_resets_and_reports_the_failure() {
    // Reserved MessagePack marker: valid JSON transport, invalid checkpoint.
    let state = serde_json::from_str("[193]").unwrap();
    let frame = render::<RichState>(Request {
        state,
        ..Default::default()
    })
    .unwrap();
    assert!(frame.reset.is_some());
    assert!(
        frame
            .state
            .decode::<RichState>()
            .unwrap()
            .entries
            .is_empty()
    );
}

#[test]
fn encoded_nil_is_not_an_absent_checkpoint() {
    let state = StateSnapshot::encode(&()).unwrap();
    assert!(!state.is_empty());
    assert_eq!(state.as_bytes(), &[0xc0]);
    state.decode::<()>().unwrap();
    assert!(StateSnapshot::default().is_empty());
}
