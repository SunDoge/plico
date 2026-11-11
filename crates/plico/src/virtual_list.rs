use crate::{Animation, Key, Motion, UiContext, widgets::column};
use std::ops::Range;

// Variable-height indexing follows MyGO ui/list.go's sparse 64-row blocks:
// https://github.com/egoist/mygo (MIT), revision
// 6bd276dda679449336f68277eaaa988e1fadc54b. See licenses/MyGo-MIT.txt.
const HEIGHT_BLOCK: usize = 64;
const KEY_SEARCH: usize = 1_000;
const HEADER_SEARCH: usize = 100_000;
const FOLLOW_CHANNEL: &str = "virtual-list.follow-end";

#[derive(Clone, Debug, Default, PartialEq)]
struct HeaderCache {
    row: Option<usize>,
    from: usize,
    stop: usize,
    valid: bool,
}

impl HeaderCache {
    fn invalidate(&mut self) {
        self.valid = false;
    }
}

// This is MyGO ListState.headerAbove translated to indices and Option. The
// cache makes the common steady-state lookup constant time, while the initial
// or invalidated search remains bounded for pathological data sets.
fn header_above(
    cache: &mut HeaderCache,
    is_header: &mut impl FnMut(usize) -> bool,
    index: usize,
    count: usize,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let index = index.min(count - 1);
    let cached_row_is_valid = cache.row.is_none_or(|row| row < count && is_header(row));
    if cache.valid && cached_row_is_valid {
        if index >= cache.from && index - cache.from <= HEADER_SEARCH {
            for row in (cache.from.saturating_add(1)..=index).rev() {
                if is_header(row) {
                    *cache = HeaderCache {
                        row: Some(row),
                        from: index,
                        stop: row,
                        valid: true,
                    };
                    return Some(row);
                }
            }
            cache.from = index;
            return cache.row;
        }
        if index >= cache.stop && (cache.row.is_some() || cache.stop == 0) {
            return cache.row;
        }
    }
    let stop = index.saturating_sub(HEADER_SEARCH);
    for row in (stop..=index).rev() {
        if is_header(row) {
            *cache = HeaderCache {
                row: Some(row),
                from: index,
                stop: row,
                valid: true,
            };
            return Some(row);
        }
    }
    *cache = HeaderCache {
        row: None,
        from: index,
        stop,
        valid: true,
    };
    None
}

#[derive(Clone, Debug, PartialEq)]
struct HeightBlock {
    values: [f32; HEIGHT_BLOCK],
    known: u64,
    sum: f64,
}

impl Default for HeightBlock {
    fn default() -> Self {
        Self {
            values: [0.; HEIGHT_BLOCK],
            known: 0,
            sum: 0.,
        }
    }
}

#[derive(Clone, Debug)]
struct VariableHeights {
    count: usize,
    ids: Vec<usize>,
    blocks: Vec<HeightBlock>,
    estimate: f32,
    prefix_sum: Vec<f64>,
    prefix_count: Vec<usize>,
    dirty: bool,
}

impl PartialEq for VariableHeights {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count
            && self.ids == other.ids
            && self.blocks == other.blocks
            && self.estimate == other.estimate
    }
}

impl VariableHeights {
    fn new(count: usize, estimate: f32) -> Self {
        Self {
            count,
            ids: Vec::new(),
            blocks: Vec::new(),
            estimate,
            prefix_sum: vec![0.],
            prefix_count: vec![0],
            dirty: false,
        }
    }

    fn block(&self, id: usize) -> Result<usize, usize> {
        self.ids.binary_search(&id)
    }

    fn index(&mut self) {
        if !self.dirty && self.prefix_sum.len() == self.blocks.len() + 1 {
            return;
        }
        self.prefix_sum.clear();
        self.prefix_count.clear();
        self.prefix_sum.push(0.);
        self.prefix_count.push(0);
        for block in &self.blocks {
            self.prefix_sum
                .push(self.prefix_sum.last().copied().unwrap_or(0.) + block.sum);
            self.prefix_count.push(
                self.prefix_count.last().copied().unwrap_or(0) + block.known.count_ones() as usize,
            );
        }
        self.dirty = false;
    }

    fn average(&mut self) -> f64 {
        self.index();
        let measured = self.prefix_count.last().copied().unwrap_or(0);
        if measured == 0 {
            f64::from(self.estimate)
        } else {
            self.prefix_sum.last().copied().unwrap_or(0.) / measured as f64
        }
    }

    fn height(&mut self, index: usize) -> f64 {
        if let Ok(at) = self.block(index / HEIGHT_BLOCK) {
            let bit = 1_u64 << (index % HEIGHT_BLOCK);
            if self.blocks[at].known & bit != 0 {
                return f64::from(self.blocks[at].values[index % HEIGHT_BLOCK]);
            }
        }
        self.average()
    }

    fn top(&mut self, index: usize) -> f64 {
        let index = index.min(self.count);
        let average = self.average();
        let block_id = index / HEIGHT_BLOCK;
        let within = index % HEIGHT_BLOCK;
        let at = self.block(block_id).unwrap_or_else(|at| at);
        let mut y = self.prefix_sum[at]
            + (block_id * HEIGHT_BLOCK - self.prefix_count[at]) as f64 * average;
        if self.ids.get(at) == Some(&block_id) {
            let block = &self.blocks[at];
            for row in 0..within {
                let bit = 1_u64 << row;
                y += if block.known & bit != 0 {
                    f64::from(block.values[row])
                } else {
                    average
                };
            }
        } else {
            y += within as f64 * average;
        }
        y
    }

    fn row_at(&mut self, y: f64) -> usize {
        if self.count == 0 {
            return 0;
        }
        let y = y.max(0.);
        let mut low = 0;
        let mut high = (self.count - 1) / HEIGHT_BLOCK;
        while low < high {
            let middle = (low + high).div_ceil(2);
            if self.top(middle * HEIGHT_BLOCK) <= y {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let mut row = low * HEIGHT_BLOCK;
        let end = self.count.min(row + HEIGHT_BLOCK);
        let mut at = self.top(row);
        while row + 1 < end {
            at += self.height(row);
            if at > y {
                break;
            }
            row += 1;
        }
        row
    }

    fn set(&mut self, index: usize, height: f32) -> bool {
        if index >= self.count || !height.is_finite() || height <= 0. {
            return false;
        }
        let block_id = index / HEIGHT_BLOCK;
        let within = index % HEIGHT_BLOCK;
        let at = match self.block(block_id) {
            Ok(at) => at,
            Err(at) => {
                self.ids.insert(at, block_id);
                self.blocks.insert(at, HeightBlock::default());
                at
            }
        };
        let block = &mut self.blocks[at];
        let bit = 1_u64 << within;
        if block.known & bit != 0 && block.values[within] == height {
            return false;
        }
        block.known |= bit;
        block.values[within] = height;
        block.sum = (0..HEIGHT_BLOCK)
            .filter(|row| block.known & (1_u64 << row) != 0)
            .map(|row| f64::from(block.values[row]))
            .sum();
        self.dirty = true;
        true
    }

    fn resize(&mut self, count: usize) {
        let block_count = count.div_ceil(HEIGHT_BLOCK);
        let keep = self.ids.partition_point(|id| *id < block_count);
        self.ids.truncate(keep);
        self.blocks.truncate(keep);
        if !count.is_multiple_of(HEIGHT_BLOCK)
            && let Ok(at) = self.block(block_count.saturating_sub(1))
        {
            let block = &mut self.blocks[at];
            block.known &= (1_u64 << (count % HEIGHT_BLOCK)) - 1;
            block.sum = (0..HEIGHT_BLOCK)
                .filter(|row| block.known & (1_u64 << row) != 0)
                .map(|row| f64::from(block.values[row]))
                .sum();
        }
        self.count = count;
        self.dirty = true;
    }

    fn shift(&mut self, delta: isize) {
        if delta == 0 || self.blocks.is_empty() {
            return;
        }
        let mut measured = Vec::new();
        for (block_id, block) in self.ids.iter().copied().zip(&self.blocks) {
            for within in 0..HEIGHT_BLOCK {
                if block.known & (1_u64 << within) != 0 {
                    measured.push((block_id * HEIGHT_BLOCK + within, block.values[within]));
                }
            }
        }
        self.ids.clear();
        self.blocks.clear();
        self.dirty = true;
        for (index, height) in measured {
            let shifted = index as isize + delta;
            if shifted >= 0 {
                let _ = self.set(shifted as usize, height);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct VariableListState {
    heights: VariableHeights,
    anchor: usize,
    anchor_key: Option<Key>,
    follow: FollowEndState,
    headers: HeaderCache,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct FollowEndState {
    extent: f32,
    viewport_height: f32,
    following: bool,
    enabled: bool,
    started: bool,
    observed_offset: Option<f32>,
    written_offset: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct FixedListState {
    follow: FollowEndState,
    anchor: usize,
    anchor_key: Option<Key>,
    count: usize,
    headers: HeaderCache,
}

impl FollowEndState {
    fn observe(&mut self, enabled: bool, observed_offset: Option<f32>) -> f32 {
        let offset = observed_offset.unwrap_or(0.);
        if enabled {
            if !self.started || !self.enabled {
                self.following = true;
            } else if observed_offset.is_some()
                && observed_offset
                    .zip(self.observed_offset)
                    .is_none_or(|(current, previous)| (current - previous).abs() > 0.01)
            {
                let was_our_write = self
                    .written_offset
                    .is_some_and(|written| (offset - written).abs() <= 0.5);
                if !was_our_write {
                    let previous_end = (self.extent - self.viewport_height).max(0.);
                    self.following = (offset - previous_end).abs() <= 0.5;
                }
            }
        } else {
            self.following = false;
        }
        self.enabled = enabled;
        if let Some(observed_offset) = observed_offset {
            self.observed_offset = Some(observed_offset);
        }
        offset
    }

    fn target(&self, extent: f32, viewport_height: f32) -> Option<f32> {
        self.following.then(|| (extent - viewport_height).max(0.))
    }

    fn suspend(&mut self) {
        self.following = false;
    }

    fn wrote(&mut self, offset: f32) {
        self.written_offset = Some(offset);
    }

    fn finish(&mut self, extent: f32, viewport_height: f32) {
        self.extent = extent;
        self.viewport_height = viewport_height;
        self.started = true;
    }
}

fn apply_follow_target(
    cx: &mut UiContext<'_>,
    state: &mut FollowEndState,
    animation: Option<Animation>,
    offset: f32,
    extent: f32,
    viewport_height: f32,
) -> f32 {
    let Some(target) = state.target(extent, viewport_height) else {
        if animation.is_some() {
            cx.jump(FOLLOW_CHANNEL, offset);
        }
        return offset;
    };
    let next = animation.map_or(target, |animation| {
        cx.animate_to(FOLLOW_CHANNEL, target, animation)
    });
    if (next - offset).abs() > 0.01 {
        cx.scroll_to_y(next);
    }
    state.wrote(next);
    next
}

fn find_key_near<K: Into<Key>>(
    key: &mut impl FnMut(usize) -> K,
    needle: Key,
    near: usize,
    count: usize,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let near = near.min(count - 1);
    for distance in 0..=KEY_SEARCH {
        let low = near.checked_sub(distance);
        let high = near.checked_add(distance).filter(|index| *index < count);
        if let Some(index) = low
            && key(index).into() == needle
        {
            return Some(index);
        }
        if distance > 0
            && let Some(index) = high
            && key(index).into() == needle
        {
            return Some(index);
        }
        if low.is_none() && high.is_none() {
            break;
        }
    }
    None
}

impl VariableListState {
    fn new(count: usize, estimate: f32) -> Self {
        Self {
            heights: VariableHeights::new(count, estimate),
            anchor: 0,
            anchor_key: None,
            follow: FollowEndState::default(),
            headers: HeaderCache::default(),
        }
    }
}

/// Fixed-height list. Rows outside the window are unmounted; keep their state in
/// the application, keyed by business identity. Heights are logical pixels.
pub struct VirtualList<'u, 'e> {
    layout: crate::Layout<'u, 'e>,
    count: usize,
    row_height: f32,
    height: f32,
    fill: bool,
    overscan: usize,
    reveal: Option<usize>,
    follow_end: bool,
    follow_animation: Option<Animation>,
}
#[derive(Debug, Clone)]
pub struct VirtualListResponse {
    pub rendered: Range<usize>,
    pub offset: f32,
}
impl<'u, 'e> crate::ImplicitKey for VirtualList<'u, 'e> {
    fn key_of(&self) -> Key {
        self.layout.key_of()
    }
    fn set_key(mut self, key: Key) -> Self {
        self.layout = self.layout.set_key(key);
        self
    }
}
impl<'u, 'e> VirtualList<'u, 'e> {
    pub(crate) fn new(cx: &'u mut UiContext<'e>, count: usize, row_height: f32) -> Self {
        assert!(
            row_height.is_finite() && row_height > 0.,
            "row height must be finite and positive"
        );
        assert!(
            (count as f64 * row_height as f64) <= f32::MAX as f64,
            "list extent exceeds layout coordinates"
        );
        Self {
            layout: column(cx),
            count,
            row_height,
            height: 240.,
            fill: false,
            overscan: 2,
            reveal: None,
            follow_end: false,
            follow_animation: None,
        }
    }
    pub fn height(mut self, height: f32) -> Self {
        assert!(
            height.is_finite() && height > 0.,
            "list height must be finite and positive"
        );
        self.height = height;
        self
    }
    /// Grow to fill the parent's remaining height instead of a fixed viewport.
    /// Windowing uses the viewport height observed on the previous frame; the
    /// `height` value remains the fallback until the host reports one.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }
    /// Reveal a row without moving keyboard focus. Pass Some only on selection
    /// changes; None leaves wheel/trackpad scrolling under the user's control.
    pub fn reveal(mut self, index: Option<usize>) -> Self {
        self.reveal = index;
        self
    }
    pub fn overscan(mut self, rows: usize) -> Self {
        self.overscan = rows;
        self
    }

    /// Start at the end and follow appended rows until the user scrolls away.
    pub fn follow_end(mut self) -> Self {
        self.follow_end = true;
        self
    }

    /// Follow the end through a shared keyed animation channel. Retargeting an
    /// in-flight append remains continuous; reduced motion snaps to the end.
    pub fn follow_end_with(mut self, animation: Animation) -> Self {
        self.follow_end = true;
        self.follow_animation = Some(animation);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.layout = self.layout.width(width);
        self
    }
    /// Size the viewport across its parent, including zero-basis growing cells.
    pub fn width_percent(mut self, width: f32) -> Self {
        self.layout = self.layout.width_percent(width);
        self
    }
    /// Business keys must be unique among siblings. The row callback runs only
    /// for the rendered range; each row has a clipped, non-shrinking fixed box.
    pub fn rows<K: Into<Key>>(
        self,
        key: impl FnMut(usize) -> K,
        build: impl FnMut(&mut UiContext<'_>, usize),
    ) -> VirtualListResponse {
        self.rows_impl(key, None::<fn(usize) -> bool>, build)
    }

    /// Virtualize sectioned rows while keeping the current section header
    /// mounted and pinned. A following header pushes the previous one away.
    /// Header lookup is cached and scans at most 100,000 rows after invalidation.
    pub fn rows_with_headers<K: Into<Key>>(
        self,
        key: impl FnMut(usize) -> K,
        is_header: impl FnMut(usize) -> bool,
        build: impl FnMut(&mut UiContext<'_>, usize),
    ) -> VirtualListResponse {
        self.rows_impl(key, Some(is_header), build)
    }

    fn rows_impl<K: Into<Key>>(
        self,
        mut key: impl FnMut(usize) -> K,
        mut is_header: Option<impl FnMut(usize) -> bool>,
        mut build: impl FnMut(&mut UiContext<'_>, usize),
    ) -> VirtualListResponse {
        let Self {
            layout,
            count,
            row_height,
            height,
            fill,
            overscan,
            reveal,
            follow_end,
            follow_animation,
        } = self;
        layout
            .height(if fill { 0. } else { height })
            .grow(if fill { 1. } else { 0. })
            .shrink(0.)
            .scroll_y(true)
            .motion(Motion::None)
            .children(|cx| {
                let state = cx.state(FixedListState::default);
                let mut list = state.get();
                if list.count != count {
                    list.count = count;
                    list.headers.invalidate();
                }
                let height = if fill {
                    cx.engine
                        .scroll_viewport(cx.parent)
                        .map_or(height, |v| v[1].max(1.))
                } else {
                    height
                };
                let viewport = cx.engine.scroll_viewport(cx.parent);
                let observed_offset = viewport.map(|value| value[0]);
                let extent = count as f32 * row_height;
                let mut offset = list
                    .follow
                    .observe(follow_end, observed_offset)
                    .clamp(0., (extent - height).max(0.));
                if let Some(anchor_key) = list.anchor_key.filter(|_| count > 0) {
                    let moved_anchor = find_key_near(&mut key, anchor_key, list.anchor, count);
                    if let Some(new_anchor) = moved_anchor.filter(|index| *index != list.anchor) {
                        offset = (offset
                            + (new_anchor as f64 - list.anchor as f64) as f32 * row_height)
                            .clamp(0., (extent - height).max(0.));
                        list.anchor = new_anchor;
                        list.headers.invalidate();
                        list.follow.wrote(offset);
                        cx.scroll_to_y(offset);
                    }
                }
                offset = apply_follow_target(
                    cx,
                    &mut list.follow,
                    follow_animation,
                    offset,
                    extent,
                    height,
                );
                if let Some(index) = reveal.filter(|i| *i < count) {
                    let top = index as f32 * row_height;
                    let bottom = top + row_height;
                    let target = if top < offset {
                        top
                    } else if bottom > offset + height {
                        bottom - height
                    } else {
                        offset
                    };
                    if target != offset {
                        list.follow.suspend();
                        offset = target.clamp(0., (count as f32 * row_height - height).max(0.));
                        if follow_animation.is_some() {
                            cx.jump(FOLLOW_CHANNEL, offset);
                        }
                        list.follow.wrote(offset);
                        cx.engine.scroll_requests.insert(cx.parent, offset);
                        cx.engine.set_scroll_viewport(cx.parent, offset, height);
                    }
                }
                list.follow.finish(extent, height);
                let rendered = crate::VisibleRegion::from_rect([0., offset, 1., offset + height])
                    .rows(count, row_height, overscan);
                let (start, end) = (rendered.start, rendered.end);
                let anchor = if count == 0 {
                    0
                } else {
                    (offset / row_height).floor() as usize
                }
                .min(count.saturating_sub(1));
                list.anchor = anchor;
                list.anchor_key = (anchor < count).then(|| key(anchor).into());
                let pinned_header = is_header
                    .as_mut()
                    .and_then(|header| header_above(&mut list.headers, header, anchor, count));
                // `list` only records the observation and anchor already used
                // above. Like MyGO's ListState, retaining that frame-local
                // bookkeeping must not schedule a second declaration.
                state.commit(list);
                let mut row = |cx: &mut UiContext<'_>, index: usize| {
                    let layout = column(cx)
                        .key(key(index))
                        .height(row_height)
                        .width_percent(100.)
                        .shrink(0.)
                        .clip(true)
                        .motion(Motion::None);
                    let layout = if is_header.as_mut().is_some_and(|header| header(index)) {
                        layout.sticky(0.).sticky_push(true)
                    } else {
                        layout
                    };
                    layout.children(|cx| build(cx, index));
                };
                if let Some(header) = pinned_header.filter(|header| *header < start) {
                    column(cx)
                        .key("vlist-before-header")
                        .height(header as f32 * row_height)
                        .shrink(0.)
                        .children(|_| {});
                    cx.keyed("vlist-pinned-header", |cx| row(cx, header));
                    column(cx)
                        .key("vlist-before-rows")
                        .height((start - header - 1) as f32 * row_height)
                        .shrink(0.)
                        .children(|_| {});
                } else {
                    column(cx)
                        .key("vlist-before")
                        .height(start as f32 * row_height)
                        .shrink(0.)
                        .children(|_| {});
                }
                // Separate business keys from the two internal spacer identities.
                cx.keyed("vlist-rows", |cx| {
                    for index in start..end {
                        row(cx, index);
                    }
                });
                column(cx)
                    .key("vlist-after")
                    .height((count - end) as f32 * row_height)
                    .shrink(0.)
                    .children(|_| {});
                VirtualListResponse {
                    rendered: start..end,
                    offset,
                }
            })
            .inner
    }
}

/// Variable-height virtual list with sparse host measurements.
///
/// Visible rows are measured after layout. Unseen rows use the average of the
/// measured rows, falling back to `estimated_row_height` until the first
/// measurement. The scroll anchor is corrected when overscan rows above the
/// viewport resolve to a different height.
pub struct VariableVirtualList<'u, 'e> {
    layout: crate::Layout<'u, 'e>,
    count: usize,
    estimated_row_height: f32,
    height: f32,
    fill: bool,
    overscan: usize,
    reveal: Option<usize>,
    follow_end: bool,
    follow_animation: Option<Animation>,
}

impl<'u, 'e> crate::ImplicitKey for VariableVirtualList<'u, 'e> {
    fn key_of(&self) -> Key {
        self.layout.key_of()
    }

    fn set_key(mut self, key: Key) -> Self {
        self.layout = self.layout.set_key(key);
        self
    }
}

impl<'u, 'e> VariableVirtualList<'u, 'e> {
    pub(crate) fn new(cx: &'u mut UiContext<'e>, count: usize, estimated_row_height: f32) -> Self {
        assert!(
            estimated_row_height.is_finite() && estimated_row_height > 0.,
            "estimated row height must be finite and positive"
        );
        Self {
            layout: column(cx),
            count,
            estimated_row_height,
            height: 240.,
            fill: false,
            overscan: 2,
            reveal: None,
            follow_end: false,
            follow_animation: None,
        }
    }

    pub fn height(mut self, height: f32) -> Self {
        assert!(
            height.is_finite() && height > 0.,
            "list height must be finite and positive"
        );
        self.height = height;
        self
    }

    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn reveal(mut self, index: Option<usize>) -> Self {
        self.reveal = index;
        self
    }

    pub fn overscan(mut self, rows: usize) -> Self {
        self.overscan = rows;
        self
    }

    /// Start at the end and keep following appended or growing rows while the
    /// user remains there. Scrolling away from the end suspends following;
    /// scrolling back resumes it.
    pub fn follow_end(mut self) -> Self {
        self.follow_end = true;
        self
    }

    /// Animated variant of [`Self::follow_end`].
    pub fn follow_end_with(mut self, animation: Animation) -> Self {
        self.follow_end = true;
        self.follow_animation = Some(animation);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.layout = self.layout.width(width);
        self
    }

    pub fn width_percent(mut self, width: f32) -> Self {
        self.layout = self.layout.width_percent(width);
        self
    }

    pub fn rows<K: Into<Key>>(
        self,
        key: impl FnMut(usize) -> K,
        build: impl FnMut(&mut UiContext<'_>, usize),
    ) -> VirtualListResponse {
        self.rows_impl(key, None::<fn(usize) -> bool>, build)
    }

    /// Variable-height counterpart of [`VirtualList::rows_with_headers`].
    /// The pinned header participates in sparse measurement just like a visible
    /// row, so its learned height remains attached to its business position.
    pub fn rows_with_headers<K: Into<Key>>(
        self,
        key: impl FnMut(usize) -> K,
        is_header: impl FnMut(usize) -> bool,
        build: impl FnMut(&mut UiContext<'_>, usize),
    ) -> VirtualListResponse {
        self.rows_impl(key, Some(is_header), build)
    }

    fn rows_impl<K: Into<Key>>(
        self,
        mut key: impl FnMut(usize) -> K,
        mut is_header: Option<impl FnMut(usize) -> bool>,
        mut build: impl FnMut(&mut UiContext<'_>, usize),
    ) -> VirtualListResponse {
        let Self {
            layout,
            count,
            estimated_row_height,
            height,
            fill,
            overscan,
            reveal,
            follow_end,
            follow_animation,
        } = self;
        layout
            .height(if fill { 0. } else { height })
            .grow(if fill { 1. } else { 0. })
            .shrink(0.)
            .scroll_y(true)
            .motion(Motion::None)
            .children(|cx| {
                let state = cx.state(|| VariableListState::new(count, estimated_row_height));
                let mut list = state.get();
                list.heights.estimate = estimated_row_height;
                let viewport_height = if fill {
                    cx.scroll_viewport()
                        .map_or(height, |value| value[1].max(1.))
                } else {
                    height
                };
                let viewport = cx.scroll_viewport();
                let observed_offset = viewport.map(|value| value[0]);
                let mut offset = list.follow.observe(follow_end, observed_offset);

                // Like MyGO, keep the item at the viewport's top stable when
                // rows are inserted or removed above it. Searching is bounded
                // so a pathological key callback cannot turn a frame into a
                // full-list scan.
                let old_count = list.heights.count;
                if old_count != count {
                    list.headers.invalidate();
                }
                let old_anchor = list.anchor.min(old_count.saturating_sub(1));
                let old_anchor_top = list.heights.top(old_anchor) as f32;
                let moved_anchor = list
                    .anchor_key
                    .and_then(|anchor_key| find_key_near(&mut key, anchor_key, list.anchor, count));
                if let Some(new_anchor) = moved_anchor.filter(|index| *index != list.anchor) {
                    list.heights.resize(count.max(old_count));
                    list.heights
                        .shift(new_anchor as isize - list.anchor as isize);
                    list.heights.resize(count);
                    list.anchor = new_anchor;
                    list.headers.invalidate();
                    let new_anchor_top = list.heights.top(new_anchor) as f32;
                    offset = (offset + new_anchor_top - old_anchor_top).max(0.);
                    cx.scroll_to_y(offset);
                } else {
                    list.heights.resize(count);
                    list.anchor = list.anchor.min(count.saturating_sub(1));
                }

                let extent = list.heights.top(count) as f32;
                offset = offset.clamp(0., (extent - viewport_height).max(0.));
                offset = apply_follow_target(
                    cx,
                    &mut list.follow,
                    follow_animation,
                    offset,
                    extent,
                    viewport_height,
                );
                if let Some(index) = reveal.filter(|index| *index < count) {
                    let top = list.heights.top(index) as f32;
                    let bottom = list.heights.top(index + 1) as f32;
                    let target = if top < offset {
                        top
                    } else if bottom > offset + viewport_height {
                        bottom - viewport_height
                    } else {
                        offset
                    };
                    if target != offset {
                        list.follow.suspend();
                        offset = target.clamp(0., (extent - viewport_height).max(0.));
                        if follow_animation.is_some() {
                            cx.jump(FOLLOW_CHANNEL, offset);
                        }
                        list.follow.wrote(offset);
                        cx.scroll_to_y(offset);
                    }
                }
                let visible_start = list.heights.row_at(f64::from(offset));
                let visible_end = if count == 0 {
                    0
                } else {
                    (list.heights.row_at(f64::from(offset + viewport_height)) + 1).min(count)
                };
                let start = visible_start.saturating_sub(overscan);
                let end = visible_end.saturating_add(overscan).min(count);
                let anchor_before = list.heights.top(visible_start) as f32;
                let after = (list.heights.top(count) - list.heights.top(end)).max(0.) as f32;
                list.anchor = visible_start;
                list.anchor_key = (visible_start < count).then(|| key(visible_start).into());
                let pinned_header = is_header.as_mut().and_then(|header| {
                    header_above(&mut list.headers, header, visible_start, count)
                });

                let mut measurements = Vec::with_capacity(
                    end.saturating_sub(start) + usize::from(pinned_header.is_some()),
                );
                let mut row = |cx: &mut UiContext<'_>, index: usize| {
                    let layout = column(cx)
                        .key(key(index))
                        .width_percent(100.)
                        .shrink(0.)
                        .motion(Motion::None);
                    let layout = if is_header.as_mut().is_some_and(|header| header(index)) {
                        layout.sticky(0.).sticky_push(true)
                    } else {
                        layout
                    };
                    layout.children(|cx| {
                        if let Some([_, measured]) = cx.layout_size() {
                            measurements.push((index, measured));
                        }
                        build(cx, index);
                    });
                };
                if let Some(header) = pinned_header.filter(|header| *header < start) {
                    column(cx)
                        .key("variable-vlist-before-header")
                        .height(list.heights.top(header) as f32)
                        .shrink(0.)
                        .children(|_| {});
                    cx.keyed("variable-vlist-pinned-header", |cx| row(cx, header));
                    column(cx)
                        .key("variable-vlist-before-rows")
                        .height(
                            (list.heights.top(start) - list.heights.top(header + 1)).max(0.) as f32,
                        )
                        .shrink(0.)
                        .children(|_| {});
                } else {
                    column(cx)
                        .key("variable-vlist-before")
                        .height(list.heights.top(start) as f32)
                        .shrink(0.)
                        .children(|_| {});
                }
                cx.keyed("variable-vlist-rows", |cx| {
                    for index in start..end {
                        row(cx, index);
                    }
                });
                drop(row);
                column(cx)
                    .key("variable-vlist-after")
                    .height(after)
                    .shrink(0.)
                    .children(|_| {});

                let mut measurements_changed = false;
                if !measurements.is_empty() {
                    for (index, height) in measurements {
                        measurements_changed |= list.heights.set(index, height);
                    }
                    let anchor_after = list.heights.top(visible_start) as f32;
                    let correction = anchor_after - anchor_before;
                    if correction.abs() > 0.01 && offset > 0. {
                        cx.scroll_to_y((offset + correction).max(0.));
                    }
                }
                let measured_extent = list.heights.top(count) as f32;
                offset = apply_follow_target(
                    cx,
                    &mut list.follow,
                    follow_animation,
                    offset,
                    measured_extent,
                    viewport_height,
                );
                list.follow.finish(measured_extent, viewport_height);
                if measurements_changed {
                    state.set(list);
                } else {
                    // Scroll observations, anchors and header-search cache were
                    // already consumed by this declaration. Only new row
                    // measurements require another settle pass.
                    state.commit(list);
                }

                VirtualListResponse {
                    rendered: start..end,
                    offset,
                }
            })
            .inner
    }
}

#[cfg(test)]
mod variable_height_tests {
    use super::VariableHeights;

    #[test]
    fn sparse_measurements_drive_offsets_and_lookup() {
        let mut heights = VariableHeights::new(1_000_000, 20.);
        let _ = heights.set(0, 40.);
        let _ = heights.set(130, 80.);
        assert_eq!(heights.top(1), 40.);
        assert_eq!(heights.top(2), 100.);
        assert_eq!(heights.row_at(39.), 0);
        assert_eq!(heights.row_at(40.), 1);
        assert!(heights.blocks.len() <= 2);
    }

    #[test]
    fn resize_forgets_measurements_past_the_new_end() {
        let mut heights = VariableHeights::new(200, 20.);
        let _ = heights.set(10, 30.);
        let _ = heights.set(150, 70.);
        heights.resize(64);
        assert_eq!(heights.ids, vec![0]);
        assert_eq!(heights.top(64), 1_920.);
    }

    #[test]
    fn shift_moves_sparse_measurements_with_their_rows() {
        let mut heights = VariableHeights::new(200, 20.);
        let _ = heights.set(10, 30.);
        let _ = heights.set(70, 50.);
        heights.resize(205);
        heights.shift(5);
        assert_eq!(heights.height(15), 30.);
        assert_eq!(heights.height(75), 50.);
        heights.shift(-10);
        assert_eq!(heights.height(5), 30.);
        assert_eq!(heights.height(65), 50.);
    }
}
