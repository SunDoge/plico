use plico_core::{Clock, Engine, VirtualClock};
use plico_markdown::{Document, RevealUnit, Stream};
use std::time::Duration;

#[test]
fn incremental_parse_matches_full_parse_at_every_utf8_boundary() {
    for input in [
        "# Heading\n\nA **bold** and *italic* 中文 paragraph.\n\n```rust\nlet a = 2;\n\nlet b = 3;\n```\n\nLast.",
        "- first\n\n  continuation\n\n- second\n\nAfter list\n---\n\nend",
        "[example][id]\n\n[id]: https://example.com\n",
        "> quote\n>\n> more\n\nnew\n\n![alt](image.png)",
    ] {
        let mut cached = Document::default();
        for end in input.char_indices().map(|(i, _)| i).chain([input.len()]) {
            let prefix = &input[..end];
            cached.update(prefix, Duration::ZERO);
            let mut full = Document::default();
            full.update(prefix, Duration::ZERO);
            assert_eq!(cached.blocks(), full.blocks(), "prefix: {prefix:?}");
        }
    }
}
#[test]
fn cache_keeps_prefix_and_skips_idle_work_and_handles_replacement() {
    let mut doc = Document::default();
    let prefix = "A complete paragraph.\n\n".repeat(200);
    doc.update(&format!("{prefix}tail"), Duration::ZERO);
    let bytes = doc.parsed_bytes;
    let first = doc.blocks()[0].text.as_ptr();
    doc.update(&format!("{prefix}tail more"), Duration::from_secs(1));
    assert_eq!(doc.blocks()[0].text.as_ptr(), first);
    assert!(doc.parsed_bytes - bytes < 30);
    let bytes = doc.parsed_bytes;
    doc.update(&format!("{prefix}tail more"), Duration::from_secs(2));
    assert_eq!(doc.parsed_bytes, bytes);
    doc.update("Replacement", Duration::from_secs(3));
    assert_eq!(doc.blocks().len(), 1);
    assert_eq!(doc.blocks()[0].text, "Replacement");
}
#[test]
fn finish_drains_reveal_graphemes_and_reload_rebuilds_cache() {
    let clock = VirtualClock::default();
    let mut s = Stream::default();
    s.push("e\u{301}👩‍💻中文 **bold**");
    s.tick(clock.now());
    clock.advance(Duration::from_millis(29)).unwrap();
    s.tick(clock.now());
    assert_eq!(s.shown(), "e\u{301}");
    s.finish();
    assert!(s.is_revealing());
    for _ in 0..100 {
        clock.advance(Duration::from_millis(16)).unwrap();
        s.tick(clock.now());
    }
    assert_eq!(s.shown(), s.received());
    assert!(!s.is_revealing());
    assert!(!s.is_receiving());
    let json = serde_json::to_string(&s).unwrap();
    let mut reloaded: Stream = serde_json::from_str(&json).unwrap();
    reloaded.tick(clock.now());
    assert_eq!(reloaded.document().source(), s.document().source());
}
#[test]
fn line_mode_waits_for_boundary_and_reduce_motion_is_immediate() {
    let mut s = Stream::default();
    s.unit = RevealUnit::Line;
    s.push("unfinished");
    s.tick(Duration::ZERO);
    s.tick(Duration::from_secs(1));
    assert_eq!(s.shown(), "");
    s.push("\nnext");
    s.tick(Duration::from_secs(2));
    assert_eq!(s.shown(), "unfinished\n");
    s.reduced_motion = true;
    s.tick(Duration::from_secs(3));
    assert_eq!(s.shown(), s.received());
}
#[test]
fn stable_blocks_reuse_nodes_and_animations_eventually_sleep() {
    let mut s = Stream::default();
    s.unit = RevealUnit::AtOnce;
    s.push("One **bold** paragraph.\n\nTail");
    s.finish();
    let mut e = Engine::default();
    e.frame_declarations(|cx| s.show(cx)).unwrap();
    let first = e.snapshot();
    e.set_time(plico_core::FrameTime {
        now: Duration::from_secs(1),
        delta: Duration::from_secs(1),
    });
    let stats = e.frame_declarations(|cx| s.show(cx)).unwrap();
    assert_eq!(stats.created, 0);
    assert!(!e.changes().contains(plico_core::Changes::ANIMATE));
    assert_eq!(first[0].children[0].key, e.snapshot()[0].children[0].key);
}
