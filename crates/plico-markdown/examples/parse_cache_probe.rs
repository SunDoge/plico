//! Finished-message parsing cache versus repeated declaration. No renderer/memo.
use plico::Engine;
use plico_markdown::Stream;
use std::time::Instant;
fn message(source: &str) -> Stream {
    let mut stream = Stream::default();
    stream.reduced_motion = true;
    stream.push(source);
    stream.finish();
    stream
}
fn main() {
    println!("mode,messages,p50_us,p95_us,visited,parsed_bytes_per_frame");
    for count in [10, 100, 1_000] {
        let source = "## Finished reply\n\nA **formatted** paragraph with `inline code` and 中文文字.\n\nAnother paragraph that stays unchanged while a new reply arrives.\n\n```rust\nlet answer = 42;\n```\n";
        for fresh in [false, true] {
            let mut engine = Engine::default();
            let mut streams: Vec<_> = (0..count).map(|_| message(source)).collect();
            let mut samples = Vec::new();
            let mut parsed = 0;
            let mut visited = 0;
            for frame in 0..136 {
                let start = Instant::now();
                let mut frame_parsed = 0;
                let stats = engine
                    .frame_declarations(|cx| {
                        for (id, stream) in streams.iter_mut().enumerate() {
                            cx.keyed(id, |cx| {
                                if fresh {
                                    let mut temporary = message(source);
                                    temporary.show(cx);
                                    frame_parsed += temporary.document().parsed_bytes;
                                } else {
                                    let before = stream.document().parsed_bytes;
                                    stream.show(cx);
                                    frame_parsed += stream.document().parsed_bytes - before;
                                }
                            });
                        }
                    })
                    .unwrap();
                let us = start.elapsed().as_secs_f64() * 1e6;
                if frame >= 16 {
                    assert_eq!((stats.created, stats.removed, stats.text_writes), (0, 0, 0));
                    samples.push(us);
                    parsed += frame_parsed;
                    visited += stats.visited;
                }
            }
            if !fresh {
                assert_eq!(parsed, 0);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "{},{count},{:.3},{:.3},{},{:.0}",
                if fresh {
                    "reparse_and_declare"
                } else {
                    "cached_parse_still_declare"
                },
                samples[60],
                samples[114],
                visited / 120,
                parsed as f64 / 120.
            );
        }
    }
}
