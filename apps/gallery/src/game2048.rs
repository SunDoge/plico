//! 2048 rules and clock-driven tile presentation, inspired by the local MyGo demo.
use plico_core::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
const INK: Color = Color::Literal(0x776e65);
const PAPER: Color = Color::Literal(0xfaf8ef);
const BOARD: Color = Color::Literal(0xbbada0);
const LIGHT: Color = Color::Literal(0xf9f6f2);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Right,
    Down,
    Left,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub id: u64,
    pub value: u32,
    pub row: usize,
    pub col: usize,
    from: [f32; 2],
    fresh: bool,
    merged: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Game {
    pub tiles: Vec<Tile>,
    ghosts: Vec<Tile>,
    pub score: u64,
    pub best: u64,
    pub won: bool,
    pub keep_playing: bool,
    pub over: bool,
    next_id: u64,
    rng: u64,
    started: Duration,
    pub time: Duration,
    gained: u64,
}
impl Default for Game {
    fn default() -> Self {
        let mut game = Self {
            tiles: vec![],
            ghosts: vec![],
            score: 0,
            best: 0,
            won: false,
            keep_playing: false,
            over: false,
            next_id: 0,
            rng: 0x2048_504c_4943_4f21,
            started: Duration::ZERO,
            time: Duration::ZERO,
            gained: 0,
        };
        game.spawn();
        game.spawn();
        game
    }
}
impl Game {
    pub fn from_cells(cells: [[u32; 4]; 4]) -> Self {
        let mut game = Self::default();
        game.tiles.clear();
        game.next_id = 0;
        for (r, row) in cells.iter().enumerate() {
            for (c, &value) in row.iter().enumerate() {
                if value != 0 {
                    assert!(value >= 2 && value.is_power_of_two() && value <= (1 << 30));
                    game.next_id += 1;
                    game.tiles.push(Tile {
                        id: game.next_id,
                        value,
                        row: r,
                        col: c,
                        from: [c as f32, r as f32],
                        fresh: false,
                        merged: false,
                    });
                    game.won |= value >= 2048;
                }
            }
        }
        game.over = !game.moves_available();
        game
    }
    pub fn cells(&self) -> [[u32; 4]; 4] {
        let mut cells = [[0; 4]; 4];
        for t in &self.tiles {
            cells[t.row][t.col] = t.value;
        }
        cells
    }
    pub fn terminated(&self) -> bool {
        self.over || (self.won && !self.keep_playing)
    }
    fn random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
    fn spawn(&mut self) {
        let cells = self.cells();
        let empty: Vec<_> = (0..16).filter(|i| cells[i / 4][i % 4] == 0).collect();
        if empty.is_empty() {
            return;
        }
        let cell = empty[self.random() as usize % empty.len()];
        let value = if self.random().is_multiple_of(10) {
            4
        } else {
            2
        };
        self.next_id += 1;
        self.tiles.push(Tile {
            id: self.next_id,
            value,
            row: cell / 4,
            col: cell % 4,
            from: [(cell % 4) as f32, (cell / 4) as f32],
            fresh: true,
            merged: false,
        });
    }
    pub fn restart(&mut self) {
        let best = self.best;
        let rng = self.random();
        let time = self.time;
        *self = Self {
            best,
            rng,
            time,
            started: time,
            tiles: vec![],
            ghosts: vec![],
            ..Self::default()
        };
        self.spawn();
        self.spawn();
    }
    pub fn moves_available(&self) -> bool {
        let b = self.cells();
        (0..4).any(|r| {
            (0..4).any(|c| {
                b[r][c] == 0
                    || (r < 3 && b[r][c] == b[r + 1][c])
                    || (c < 3 && b[r][c] == b[r][c + 1])
            })
        })
    }
    pub fn move_dir(&mut self, direction: Direction) -> bool {
        if self.terminated() {
            return false;
        }
        let before = self.cells();
        let elapsed = self.time.saturating_sub(self.started).as_secs_f32();
        let mut old = self.tiles.clone();
        for t in &mut old {
            t.from = position(t, elapsed);
            t.fresh = false;
            t.merged = false;
        }
        let mut result = Vec::new();
        let mut ghosts = Vec::new();
        let mut gained = 0;
        for line in 0..4 {
            let coord = |offset| match direction {
                Direction::Left => (line, offset),
                Direction::Right => (line, 3 - offset),
                Direction::Up => (offset, line),
                Direction::Down => (3 - offset, line),
            };
            let mut items: Vec<_> = (0..4)
                .filter_map(|offset| {
                    let (r, c) = coord(offset);
                    old.iter().find(|t| t.row == r && t.col == c).cloned()
                })
                .collect();
            let mut at = 0;
            while !items.is_empty() {
                let mut first = items.remove(0);
                let (r, c) = coord(at);
                if items.first().is_some_and(|next| next.value == first.value)
                    && first.value <= (1 << 30)
                {
                    let mut second = items.remove(0);
                    first.row = r;
                    first.col = c;
                    second.row = r;
                    second.col = c;
                    let value = first.value * 2;
                    ghosts.extend([first, second]);
                    self.next_id += 1;
                    result.push(Tile {
                        id: self.next_id,
                        value,
                        row: r,
                        col: c,
                        from: [c as f32, r as f32],
                        fresh: true,
                        merged: true,
                    });
                    gained += u64::from(value);
                } else {
                    first.row = r;
                    first.col = c;
                    result.push(first);
                }
                at += 1;
            }
        }
        let mut after = [[0; 4]; 4];
        for tile in &result {
            after[tile.row][tile.col] = tile.value;
        }
        if before == after {
            return false;
        }
        self.tiles = result;
        self.ghosts = ghosts;
        self.score += gained;
        self.best = self.best.max(self.score);
        self.gained = gained;
        self.won |= self.tiles.iter().any(|t| t.value >= 2048);
        self.started = self.time;
        self.spawn();
        self.over = !self.moves_available();
        true
    }
    pub fn key(&mut self, key: &str) -> bool {
        let direction = match key {
            "ArrowUp" | "w" | "W" | "k" | "K" => Direction::Up,
            "ArrowRight" | "d" | "D" | "l" | "L" => Direction::Right,
            "ArrowDown" | "s" | "S" | "j" | "J" => Direction::Down,
            "ArrowLeft" | "a" | "A" | "h" | "H" => Direction::Left,
            "r" | "R" => {
                self.restart();
                return true;
            }
            _ => return false,
        };
        self.move_dir(direction);
        true
    }
    #[component]
    #[rustfmt::skip]
    pub fn show(&mut self, cx: &mut UiContext) {
        self.time = cx.now();
        let [width, height] = cx.viewport_size();
        let size = (width - 300.).min(height - 390.).clamp(220., 400.);
        let scale = size / 500.;
        let gap = 15. * scale;
        let cell = (size - gap * 5.) / 4.;
        let elapsed = self.time.saturating_sub(self.started).as_secs_f32();
        if elapsed < 0.6 {
            cx.request_animation_frame();
        }
        column(cx).width_percent(100.).height(0.).grow(1.).align(Align::Center).scroll_y(true).padding(18).gap(16)
            .background(PAPER).radius(10.) + |cx|
        {
            column(cx).width(size).gap(14) + |cx| {
                row(cx).justify(Justify::SpaceBetween).align(Align::Center) + |cx| {
                    text(cx, "2048").font_size((80. * scale) as u16).weight(800).color(INK);
                    row(cx).gap(6) + |cx| {
                        score(cx, "SCORE", self.score, scale, self.gained, elapsed);
                        score(cx, "BEST", self.best, scale, 0, elapsed);
                    };
                };
                row(cx).justify(Justify::SpaceBetween).align(Align::Center).gap(10) + |cx| {
                    text(cx, "Join the numbers. Reach 2048!").font_size(13).color(INK).width(0.).grow(1.);
                    if game_button(cx, "New game") {
                        self.restart();
                        cx.request_rebuild();
                    }
                };
                column(cx).width(size).height(size).shrink(0.).radius(6.).background(BOARD) + |cx| {
                    for i in 0..16 {
                        column(cx).key(i as u64).absolute(gap + (i % 4) as f32 * (cell + gap),
                            gap + (i / 4) as f32 * (cell + gap)).width(cell).height(cell).radius(3.).background(Color::Literal(0xcdc1b4))
                        + |_cx|
                        {};
                    }
                    // A separate identity namespace lets cell and tile IDs coexist.
                    cx.keyed("tiles", |cx| {
                        if elapsed < 0.3 {
                            for tile in &self.ghosts {
                                tile_view(cx, tile, cell, gap, elapsed, true);
                            }
                        }
                        for tile in &self.tiles {
                            tile_view(cx, tile, cell, gap, elapsed, false);
                        }
                    });
                    if self.terminated() && elapsed >= 0.35 {
                        column(cx).absolute(0., 0.).width(size).height(size).radius(6.)
                            .background(Color::Literal(if self.over { 0xeee4da } else { 0xedcf72 }))
                            .align(Align::Center).justify(Justify::Center).gap(20)
                            .opacity(((elapsed - 0.35) / 0.25).clamp(0., 1.)) + |cx|
                        {
                            text(cx, if self.over { "Game over!" } else { "You win!" }).font_size(38).weight(750).color(INK);
                            row(cx).gap(10).children(|cx| {
                                if !self.over && game_button(cx, "Keep going") {
                                    self.keep_playing = true;
                                    cx.request_rebuild();
                                }
                                if game_button(cx, "Try again") {
                                    self.restart();
                                    cx.request_rebuild();
                                }
                            })
                        };
                    }
                };
                row(cx).gap(8).justify(Justify::Center) + |cx| {
                    for (label, dir) in [
                        ("←", Direction::Left),
                        ("↑", Direction::Up),
                        ("↓", Direction::Down),
                        ("→", Direction::Right),
                    ] {
                        cx.keyed(label, |cx| {
                            if game_button(cx, label) {
                                self.move_dir(dir);
                                cx.request_rebuild();
                            }
                        });
                    }
                };
                text(cx, "HOW TO PLAY").font_size(11).weight(750).color(INK);
                text(cx, "Use arrow keys, WASD or HJKL. Equal tiles merge once per move. R starts a new game.")
                    .font_size(12).color(INK);
            };
        };
    }
}
fn position(tile: &Tile, elapsed: f32) -> [f32; 2] {
    let t = (elapsed / 0.1).clamp(0., 1.);
    let t = t * t * (3. - 2. * t);
    [
        tile.from[0] + (tile.col as f32 - tile.from[0]) * t,
        tile.from[1] + (tile.row as f32 - tile.from[1]) * t,
    ]
}
#[component]
#[rustfmt::skip]
fn tile_view(cx: &mut UiContext, tile: &Tile, cell: f32, gap: f32, elapsed: f32, ghost: bool) {
    let pos = position(tile, elapsed);
    let p = ((elapsed - 0.1) / 0.2).clamp(0., 1.);
    let k = if !tile.fresh || ghost {
        1.
    } else if tile.merged {
        if p < 0.5 { p * 2.4 } else { 1.2 - (p - 0.5) * 0.4 }
    } else {
        p * p * (3. - 2. * p)
    };
    let colors = [
        0xeee4da, 0xede0c8, 0xf2b179, 0xf59563, 0xf67c5f, 0xf65e3b, 0xedcf72, 0xedcc61, 0xedc850,
        0xedc53f, 0xedc22e,
    ];
    let bg = colors.get(tile.value.ilog2().saturating_sub(1) as usize).copied().unwrap_or(0x3c3a32);
    let digits = tile.value.to_string().len();
    let font = match digits {
        1..=2 => 55.,
        3 => 45.,
        4 => 35.,
        _ => 27.,
    } * cell
        / 106.25;
    column(cx).key(tile.id).absolute(gap, gap).translate(pos[0] * (cell + gap), pos[1] * (cell + gap)).scale(k, k)
        .width(cell).height(cell).radius(3.).clip(true).justify(Justify::Center).align(Align::Center)
        .background(Color::Literal(bg)).opacity(if k == 0. { 0. } else { 1. }) + |cx|
    {
        plico_core::text!(cx, "{}", tile.value).font_size((font as u16).max(1)).weight(750)
            .color(if tile.value <= 4 { INK } else { LIGHT });
    };
}
#[component]
#[rustfmt::skip]
fn score(cx: &mut UiContext, label: &str, value: u64, scale: f32, gained: u64, elapsed: f32) {
    column(cx).width(82. * scale).padding(8).radius(3.).background(BOARD).align(Align::Center).gap(2) + |cx| {
        text(cx, label).font_size(10).weight(700).color(LIGHT);
        plico_core::text!(cx, "{value}").font_size(22).weight(750).color(LIGHT);
        if gained > 0 && elapsed < 0.6 {
            column(cx).absolute(0., 18.).translate(0., -elapsed / 0.6 * 50.).width(82. * scale).align(Align::Center)
                .opacity((1. - elapsed / 0.6).max(0.)) + |cx|
            {
                plico_core::text!(cx, "+{gained}").font_size(22).weight(750).color(INK);
            };
        }
    };
}
#[component]
fn game_button(cx: &mut UiContext, label: &str) -> bool {
    button_base(cx, label)
        .padding_x(14)
        .padding_y(8)
        .radius(3.)
        .background(Color::Literal(0x8f7a66))
        .hover_background(Color::Literal(0x9f8a76))
        .pressed_background(Color::Literal(0x7f6a56))
        .children(|cx| {
            text(cx, label).font_size(13).weight(650).color(LIGHT);
        })
        .response
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(values: [u32; 4]) -> Game {
        Game::from_cells([values, [0; 4], [0; 4], [0; 4]])
    }
    #[test]
    fn merges_once_and_scores_each_pair() {
        let mut game = row([2, 2, 4, 0]);
        assert!(game.move_dir(Direction::Left));
        assert_eq!(game.cells()[0][..2], [4, 4]);
        assert_eq!(game.score, 4);
        assert_eq!(game.tiles.len(), 3); // Two resulting tiles and one spawn.
        let mut game = row([2, 2, 2, 2]);
        game.move_dir(Direction::Right);
        assert_eq!(game.cells()[0][2..], [4, 4]);
        assert_eq!(game.score, 8);
        assert_eq!(game.ghosts.len(), 4);
    }
    #[test]
    fn blocked_move_neither_spawns_nor_consumes_rng_or_score() {
        let mut game = row([2, 4, 8, 16]);
        let before = game.clone();
        assert!(!game.move_dir(Direction::Left));
        assert_eq!(game, before);
    }
    #[test]
    fn all_four_directions_compress_correctly() {
        for (dir, r, c) in [
            (Direction::Left, 1, 0),
            (Direction::Right, 1, 3),
            (Direction::Up, 0, 1),
            (Direction::Down, 3, 1),
        ] {
            let mut game = Game::from_cells([[0; 4], [0, 8, 0, 0], [0; 4], [0; 4]]);
            let id = game.tiles[0].id;
            game.move_dir(dir);
            let tile = game.tiles.iter().find(|t| t.id == id).unwrap();
            assert_eq!((tile.row, tile.col), (r, c));
        }
    }
    #[test]
    fn win_continue_loss_and_restart_preserve_best() {
        let mut game = row([1024, 1024, 0, 0]);
        game.move_dir(Direction::Left);
        assert!(game.won && game.terminated());
        assert!(!game.move_dir(Direction::Right));
        game.keep_playing = true;
        assert!(game.move_dir(Direction::Right));
        assert_eq!(game.best, 2048);
        game.restart();
        assert_eq!((game.score, game.best, game.tiles.len()), (0, 2048, 2));
        assert!(!game.terminated());
        let mut lost = Game::from_cells([[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]]);
        assert!(lost.over && lost.terminated());
        assert!(!lost.move_dir(Direction::Up));
    }
    #[test]
    fn survives_reload_and_retargets_from_current_visual_position() {
        let mut game = row([0, 0, 0, 8]);
        game.move_dir(Direction::Left);
        game.time = Duration::from_millis(50);
        let tile = game.tiles.iter().find(|t| t.value == 8).unwrap();
        let middle = position(tile, 0.05);
        assert_eq!(middle, [1.5, 0.]);
        game.move_dir(Direction::Down);
        let tile = game.tiles.iter().find(|t| t.value == 8).unwrap();
        assert_eq!(position(tile, 0.), middle);
        let restored: Game = serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();
        assert_eq!(restored, game);
    }
}
