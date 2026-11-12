use gallery::{Gallery, Page};
use plico::{Router, VirtualClock};
use plico_desktop::Source;
use plico_renderer::{FrameSource, TextContext};
use std::{hint::black_box, time::Instant};
fn main() {
    for (name, page) in [
        ("workspace", Page::Workspace),
        ("controls", Page::Controls),
        ("layout", Page::Layout),
        ("data-table", Page::DataTable),
        ("tree", Page::TreeView),
        ("motion", Page::Motion),
    ] {
        let mut source = Source::with_clock(
            Gallery {
                router: Router::new(page),
                ..Default::default()
            },
            VirtualClock::default(),
        );
        let mut text = TextContext::new();
        for _ in 0..100 {
            source.request_rebuild();
            black_box(source.build_frame(&mut text, 1120, 820));
        }
        let mut elapsed = Vec::new();
        let mut declare = Vec::new();
        let mut checksum = 0;
        for _ in 0..3000 {
            source.request_rebuild();
            let start = Instant::now();
            let nodes = source.try_build_frame(&mut text, 1120, 820).unwrap();
            elapsed.push(start.elapsed().as_secs_f64() * 1e6);
            declare.push(source.frame_diagnostics().declaration_micros);
            checksum += nodes.len();
            black_box(nodes);
        }
        elapsed.sort_by(f64::total_cmp);
        declare.sort_unstable();
        println!(
            "{name},{:.3},{:.3},{},{checksum}",
            elapsed[1500], elapsed[2850], declare[1500]
        );
    }
}
