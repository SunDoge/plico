use plico_markdown::Stream;

#[test]
fn invalid_checkpoint_offsets_and_credit_are_rejected_before_slicing() {
    for shown in [1, 2, 7] {
        let value = serde_json::json!({ "received": "中文", "shown": shown });
        assert!(serde_json::from_value::<Stream>(value).is_err());
    }
    assert!(serde_json::from_value::<Stream>(serde_json::json!({"credit": -1.})).is_err());
    let stream: Stream =
        serde_json::from_value(serde_json::json!({"received": "中文", "shown": 3})).unwrap();
    assert_eq!(stream.shown(), "中");
    assert!(
        stream.document().blocks().is_empty(),
        "parser cache remains disposable"
    );
    let empty: Stream = serde_json::from_str("{}").unwrap();
    assert_eq!(empty.shown(), "");
}
