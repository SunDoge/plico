//! Work counters for clock-driven 2048 transforms; not a GPU benchmark.
use plico_backend::Source;
use plico_gallery::{
    Gallery, Page,
    game2048::{Direction, Game},
};
use plico_renderer::{FrameSource, TextContext};
use std::time::Duration;
fn main() {
    let clock = plico_core::VirtualClock::default();
    let game = Game::from_cells([
        [2, 2, 4, 8],
        [4, 8, 16, 32],
        [8, 16, 64, 128],
        [0, 0, 256, 512],
    ]);
    let mut source = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Game2048),
            game2048: game,
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    source.build_frame(&mut tcx, 1120, 820);
    source.view.game2048.move_dir(Direction::Left);
    let mut samples = Vec::new();
    for ms in [0, 50, 150, 250, 400, 600, 1000] {
        clock.set(Duration::from_millis(ms)).unwrap();
        source.build_frame(&mut tcx, 1120, 820);
        assert!(source.last_error.is_none());
        let s = source.adapter.stats;
        if [50, 150, 250].contains(&ms) {
            assert_eq!((s.layout_runs, s.style_writes, s.flatten_visits), (0, 0, 0));
        }
        samples.push(serde_json::json!({"ms":ms,"layout_runs":s.layout_runs,"style_writes":s.style_writes,
            "flatten_visits":s.flatten_visits,"paint_patches":s.paint_patches,"presentation_visits":s.presentation_visits,
            "text_measure_invalidations":s.text_measure_invalidations,"created":s.created,"removed":s.removed,"animating":source.has_anim()}));
    }
    println!("{}", serde_json::to_string_pretty(&samples).unwrap());
}
