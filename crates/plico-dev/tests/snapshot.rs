use plico_core::Ui;
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
    fn show(&mut self, _: &mut Ui<'_>) {}
}

#[test]
fn non_json_state_survives_the_actual_dispatch_envelope() {
    let original = RichState {
        entries: BTreeMap::from([((3, 7), "value".into())]),
        bytes: vec![0, 128, 255],
        infinity: f64::INFINITY,
        nan: f64::NAN,
    };
    assert!(serde_json::to_value(&original).is_err());
    let state = StateSnapshot::encode(&original).unwrap();
    // MessagePack bin8, length three, then the original bytes.
    assert!(
        state
            .as_bytes()
            .windows(5)
            .any(|b| b == [0xc4, 3, 0, 128, 255])
    );
    let request = Request {
        state,
        ..Default::default()
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    let reply = plico_dev::dispatch::<RichState>(bytes.into_iter().collect());
    assert_eq!(reply.status, 0);
    let bytes: Vec<_> = reply.bytes.into_iter().collect();
    let frame: plico_dev::Frame = serde_json::from_slice(&bytes).unwrap();
    assert!(frame.reset.is_none());
    let restored: RichState = frame.state.decode().unwrap();
    assert_eq!(restored.entries, original.entries);
    assert_eq!(restored.bytes, original.bytes);
    assert_eq!(restored.infinity, f64::INFINITY);
    assert!(restored.nan.is_nan());
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
