// Based on MyGO ui/list_move.go, MIT, revision
// 644caa510bb98622cf1d33db39bd2195812cb303. See licenses/MyGo-MIT.txt.

use crate::{Animation, Key, UiContext, motion::ValueChannel};
use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
};

const EPSILON: f32 = 0.5;
const MAX_ROWS: usize = 4096;

#[derive(Clone, Copy)]
struct Row {
    key: Key,
    index: usize,
    top: f32,
    height: f32,
    lag: ValueChannel,
    offset: f32,
}

#[derive(Default)]
pub(super) struct RowMoves {
    previous: HashMap<Key, Row>,
    rows: VecDeque<Row>,
    count: usize,
    offset: f32,
}

pub(super) struct Window {
    pub range: Range<usize>,
    pub count: usize,
    pub offset: f32,
    pub observed_offset: Option<f32>,
    pub viewport: f32,
    pub extent: f32,
    pub jumped: bool,
    pub following: bool,
    pub variable: bool,
}

impl RowMoves {
    pub(super) fn prepare<K: Into<Key>>(
        &mut self,
        cx: &mut UiContext<'_>,
        animation: Animation,
        window: &mut Window,
        key: &mut impl FnMut(usize) -> K,
        mut geometry: impl FnMut(usize) -> (f32, f32),
    ) {
        self.rows.clear();
        if cx.display_preferences().reduce_motion {
            self.previous.clear();
            return;
        }

        let Window {
            range,
            count,
            offset,
            observed_offset,
            viewport,
            extent,
            jumped,
            following,
            variable,
        } = window;
        let now = cx.now();
        let scroll = observed_offset.map_or(0., |observed| observed - self.offset);
        let moving_to_end = scroll.abs() > EPSILON
            && observed_offset.is_some_and(|observed| observed >= *extent - *viewport - EPSILON);
        let can_retarget =
            !*jumped && !*following && !moving_to_end && scroll.abs() <= 2. * *viewport;
        let mut changed = *count != self.count;
        let mut first_known = None;

        // Cache the keys once for declaration. Previous heights belong to the
        // item, while variable-list estimates are indexed by layout position.
        let mut next_top = None;
        for index in range.clone() {
            let key = key(index).into();
            let (top, estimated_height) = geometry(index);
            let old = self.previous.get(&key);
            let height = match old {
                Some(old) if *variable && old.index != index => old.height,
                _ => estimated_height,
            };
            let top = next_top.unwrap_or(top) - *offset;
            next_top = Some(top + *offset + height);
            let mut lag = ValueChannel::new(0., now);
            if let Some(old) = old {
                changed |= old.index != index;
                first_known.get_or_insert(self.rows.len());
                lag = old.lag;
            }
            self.rows.push_back(Row {
                key,
                index,
                top,
                height,
                lag,
                offset: 0.,
            });
        }

        let mut neighbor = None;
        let mut active = false;
        if let Some(first) = first_known {
            for row in self.rows.iter_mut().skip(first) {
                if let Some(old) = self.previous.get(&row.key) {
                    let (mut offset, mut moving) = row.lag.sample(now, 0., animation, false);
                    if changed && can_retarget {
                        let displacement = old.top - scroll - row.top;
                        if displacement.abs() > EPSILON {
                            row.lag.shift(displacement);
                            (offset, moving) = row.lag.sample(now, 0., animation, false);
                        }
                    }
                    row.offset = offset;
                    neighbor = Some(*row);
                    active |= moving;
                } else if let Some(neighbor) = neighbor {
                    row.lag = neighbor.lag;
                    row.offset = neighbor.offset;
                }
            }
            let first_known = self.rows[first];
            for row in self.rows.iter_mut().take(first) {
                row.lag = first_known.lag;
                row.offset = first_known.offset;
            }
        }

        // Extend the realized window only where moving rows uncover an edge.
        // Bound extra work for both huge moves and very small row heights.
        if active {
            while range.end < *count
                && self.rows.len() < MAX_ROWS
                && let Some(last) = self.rows.back().copied()
                && last.top + last.height + last.offset < *viewport
            {
                let index = range.end;
                self.rows.push_back(Row {
                    key: key(index).into(),
                    index,
                    top: last.top + last.height,
                    height: geometry(index).1,
                    ..last
                });
                range.end += 1;
            }
            while range.start > 0
                && self.rows.len() < MAX_ROWS
                && let Some(first) = self.rows.front().copied()
                && first.top + first.offset > 0.
            {
                let index = range.start - 1;
                let height = geometry(index).1;
                self.rows.push_front(Row {
                    key: key(index).into(),
                    index,
                    top: first.top - height,
                    height,
                    ..first
                });
                range.start -= 1;
            }
            cx.request_animation_frame();
        }
        self.previous.clear();
        self.previous
            .extend(self.rows.iter().map(|row| (row.key, *row)));
        self.count = *count;
        self.offset = *offset;
    }

    pub(super) fn row(&self, index: usize) -> Option<(Key, f32)> {
        let first = self.rows.front()?;
        let row = self.rows.get(index.checked_sub(first.index)?)?;
        Some((row.key, row.offset))
    }
}
