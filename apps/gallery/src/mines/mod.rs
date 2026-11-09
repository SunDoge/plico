//! Mines: a Rust/Plico adaptation of unixzii/graphite's MIT-licensed example.
//! See ../../docs/mines.md for provenance and replay instructions.
mod view;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    #[default]
    Ready,
    Running,
    Won,
    Lost,
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub mine: bool,
    pub adjacent: u8,
    pub open: bool,
    pub flagged: bool,
    /// Seconds on the injected clock at which the reveal wave reaches this tile.
    pub reveal_at: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mines {
    pub columns: usize,
    pub rows: usize,
    pub mine_count: usize,
    pub playing: bool,
    pub status: Status,
    pub cells: Vec<Cell>,
    pub seed: u64,
    pub cursor: usize,
    pub reduced_motion: bool,
    pub custom_mines: bool,
    pub selected_field: usize,
    pub time: Duration,
    started: Duration,
    finished: Duration,
    result_at: f64,
    exploded: Option<usize>,
    #[serde(skip)]
    hovered: Option<usize>,
    #[serde(skip)]
    pressed_cell: Option<usize>,
    #[serde(skip)]
    replace_number: bool,
    #[serde(skip)]
    fields: [String; 3],
    #[serde(skip)]
    focus_cursor: bool,
    #[serde(skip)]
    keyboard_cursor: bool,
}
impl Default for Mines {
    fn default() -> Self {
        Self {
            columns: 16,
            rows: 16,
            mine_count: 40,
            playing: false,
            status: Status::Ready,
            cells: Vec::new(),
            seed: 0x504c_4943_4f4d_494e,
            cursor: 0,
            reduced_motion: false,
            custom_mines: false,
            selected_field: 0,
            time: Duration::ZERO,
            started: Duration::ZERO,
            finished: Duration::ZERO,
            result_at: 0.,
            exploded: None,
            hovered: None,
            pressed_cell: None,
            replace_number: true,
            fields: Default::default(),
            focus_cursor: false,
            keyboard_cursor: false,
        }
    }
}
impl Mines {
    pub fn maximum_mines(columns: usize, rows: usize) -> usize {
        let n = columns.clamp(1, 64) * rows.clamp(1, 64);
        n.saturating_sub(if n > 9 { 9 } else { 1 })
    }
    pub fn suggested_mines(columns: usize, rows: usize) -> usize {
        match (columns, rows) {
            (9, 9) => 10,
            (16, 16) => 40,
            (30, 16) | (16, 30) => 99,
            _ => {
                let n = columns.clamp(1, 64) * rows.clamp(1, 64);
                let density = 0.12 + 0.09 * ((n as f64 - 81.) / 399.).clamp(0., 1.);
                ((n as f64 * density).round() as usize)
                    .max(1)
                    .min(Self::maximum_mines(columns, rows))
            }
        }
    }
    pub fn configure(&mut self, columns: usize, rows: usize, mines: usize) {
        self.columns = columns.clamp(1, 64);
        self.rows = rows.clamp(1, 64);
        self.mine_count = mines.min(Self::maximum_mines(self.columns, self.rows));
        self.restart();
    }
    pub fn restart(&mut self) {
        self.columns = self.columns.clamp(1, 64);
        self.rows = self.rows.clamp(1, 64);
        self.mine_count = self
            .mine_count
            .min(Self::maximum_mines(self.columns, self.rows));
        self.cells = vec![Cell::default(); self.columns * self.rows];
        self.status = Status::Ready;
        self.playing = true;
        self.cursor = 0;
        self.hovered = None;
        self.pressed_cell = None;
        self.keyboard_cursor = false;
        self.started = self.time;
        self.finished = self.time;
        self.result_at = 0.;
        self.exploded = None;
        // Advancing the seed gives New game a fresh field, while serialized
        // seeds and the same input sequence remain fully reproducible.
        self.random();
    }
    fn random(&mut self) -> u64 {
        if self.seed == 0 {
            self.seed = 1;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }
    fn neighbors(&self, i: usize) -> impl Iterator<Item = usize> + use<> {
        let (x, y) = (i % self.columns, i / self.columns);
        let mut out = Vec::with_capacity(8);
        for yy in y.saturating_sub(1)..=(y + 1).min(self.rows - 1) {
            for xx in x.saturating_sub(1)..=(x + 1).min(self.columns - 1) {
                let j = yy * self.columns + xx;
                if i != j {
                    out.push(j);
                }
            }
        }
        out.into_iter()
    }
    fn place(&mut self, first: usize) {
        let safe = self
            .neighbors(first)
            .chain(std::iter::once(first))
            .collect::<Vec<_>>();
        let exclude_neighbors = self.cells.len().saturating_sub(9) >= self.mine_count;
        let mut available = (0..self.cells.len())
            .filter(|&i| {
                if exclude_neighbors {
                    !safe.contains(&i)
                } else {
                    i != first
                }
            })
            .collect::<Vec<_>>();
        for n in 0..self.mine_count {
            let j = n + self.random() as usize % (available.len() - n);
            available.swap(n, j);
            self.cells[available[n]].mine = true;
        }
        for i in 0..self.cells.len() {
            self.cells[i].adjacent =
                self.neighbors(i).filter(|&j| self.cells[j].mine).count() as u8;
        }
        self.started = self.time;
        self.status = Status::Running;
    }
    pub fn flags(&self) -> usize {
        self.cells.iter().filter(|c| c.flagged).count()
    }
    pub fn elapsed(&self) -> Duration {
        match self.status {
            Status::Ready => Duration::ZERO,
            Status::Running => self.time.saturating_sub(self.started),
            _ => self.finished.saturating_sub(self.started),
        }
    }
    pub fn flag(&mut self, i: usize) -> bool {
        if !self.playing || self.ended() || i >= self.cells.len() || self.cells[i].open {
            return false;
        }
        self.cells[i].flagged = !self.cells[i].flagged;
        true
    }
    pub fn ended(&self) -> bool {
        matches!(self.status, Status::Won | Status::Lost)
    }
    pub fn reveal(&mut self, i: usize) -> bool {
        if !self.playing
            || self.ended()
            || i >= self.cells.len()
            || self.cells[i].flagged
            || self.cells[i].open
        {
            return false;
        }
        if self.status == Status::Ready {
            self.place(i);
        }
        let mut pending = vec![i];
        while let Some(j) = pending.pop() {
            if self.cells[j].open || self.cells[j].flagged {
                continue;
            }
            self.cells[j].open = true;
            self.cells[j].reveal_at = self.time.as_secs_f64()
                + if self.reduced_motion {
                    0.
                } else {
                    self.distance(i, j) * 0.03
                };
            if self.cells[j].mine {
                self.lose(j);
                return true;
            }
            if self.cells[j].adjacent == 0 {
                pending.extend(self.neighbors(j));
            }
        }
        if self.cells.iter().filter(|c| c.open && !c.mine).count()
            == self.cells.len() - self.mine_count
        {
            self.status = Status::Won;
            self.finished = self.time;
            self.result_at = self.time.as_secs_f64() + if self.reduced_motion { 0. } else { 1.3 };
            for c in &mut self.cells {
                if c.mine {
                    c.flagged = true;
                }
            }
        }
        true
    }
    pub fn chord(&mut self, i: usize) -> bool {
        if self.ended()
            || i >= self.cells.len()
            || !self.cells[i].open
            || self.cells[i].adjacent == 0
        {
            return false;
        }
        let neighbors = self.neighbors(i).collect::<Vec<_>>();
        if neighbors.iter().filter(|&&j| self.cells[j].flagged).count()
            != self.cells[i].adjacent as usize
        {
            return false;
        }
        let mut changed = false;
        for j in neighbors {
            changed |= self.reveal(j);
        }
        changed
    }
    fn distance(&self, a: usize, b: usize) -> f64 {
        ((a % self.columns) as f64 - (b % self.columns) as f64)
            .hypot((a / self.columns) as f64 - (b / self.columns) as f64)
    }
    fn lose(&mut self, origin: usize) {
        self.exploded = Some(origin);
        self.status = Status::Lost;
        self.finished = self.time;
        let mut mines = (0..self.cells.len())
            .filter(|&i| self.cells[i].mine && !self.cells[i].flagged && i != origin)
            .collect::<Vec<_>>();
        mines.sort_by(|&a, &b| {
            self.distance(origin, a)
                .total_cmp(&self.distance(origin, b))
        });
        let mut delay = 0.45;
        let mut gap: f64 = 0.26;
        for i in mines {
            self.cells[i].open = true;
            self.cells[i].reveal_at =
                self.time.as_secs_f64() + if self.reduced_motion { 0. } else { delay };
            delay += gap;
            gap = (gap * 0.84).max(0.0025);
        }
        self.result_at = self.time.as_secs_f64()
            + if self.reduced_motion {
                0.
            } else {
                delay + 0.75
            };
    }
    fn edit_number(&mut self, value: usize) {
        match self.selected_field {
            0 => self.columns = value.clamp(1, 64),
            1 => self.rows = value.clamp(1, 64),
            _ => {
                self.mine_count = value;
                self.custom_mines = true;
            }
        }
        if !self.custom_mines {
            self.mine_count = Self::suggested_mines(self.columns, self.rows);
        }
        self.mine_count = self
            .mine_count
            .min(Self::maximum_mines(self.columns, self.rows));
    }
    pub fn key(&mut self, key: &str) -> bool {
        if !self.playing {
            let current = [self.columns, self.rows, self.mine_count][self.selected_field.min(2)];
            match key {
                "ArrowLeft" => {
                    self.selected_field = (self.selected_field + 2) % 3;
                    self.replace_number = true;
                }
                "ArrowRight" => {
                    self.selected_field = (self.selected_field + 1) % 3;
                    self.replace_number = true;
                }
                "ArrowUp" => self.edit_number(current.saturating_add(1)),
                "ArrowDown" => self.edit_number(current.saturating_sub(1)),
                "Shift+ArrowUp" => self.edit_number(current.saturating_add(10)),
                "Shift+ArrowDown" => self.edit_number(current.saturating_sub(10)),
                "Enter" => self.restart(),
                "Backspace" | "Delete" => self.edit_number(current / 10),
                digit if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => {
                    let value = digit.as_bytes()[0] as usize - b'0' as usize;
                    self.edit_number(if self.replace_number {
                        value
                    } else {
                        current * 10 + value
                    });
                    self.replace_number = false;
                }
                _ => return false,
            }
            return true;
        }
        match key {
            "Escape" => {
                self.playing = false;
            }
            "r" | "R" => self.restart(),
            "f" | "F" => {
                self.flag(self.hovered.unwrap_or(self.cursor));
            }
            "ArrowLeft" => {
                self.cursor = (self.cursor / self.columns) * self.columns
                    + (self.cursor % self.columns).saturating_sub(1)
            }
            "ArrowRight" => {
                self.cursor =
                    (self.cursor + 1).min((self.cursor / self.columns + 1) * self.columns - 1)
            }
            "ArrowUp" => self.cursor = self.cursor.saturating_sub(self.columns),
            "ArrowDown" => {
                self.cursor = (self.cursor + self.columns).min(self.cells.len().saturating_sub(1))
            }
            "Enter" | " " => {
                if self.ended() {
                    self.restart();
                } else if self.cells[self.cursor].open {
                    self.chord(self.cursor);
                } else {
                    self.reveal(self.cursor);
                }
            }
            _ => return false,
        }
        if matches!(key, "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown") {
            self.focus_cursor = true;
            self.keyboard_cursor = true;
        }
        true
    }
}
