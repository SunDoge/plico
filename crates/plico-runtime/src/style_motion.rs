//! Typed property transitions, owned by the host and keyed by retained Entity.
use plico_core::{Color, Entity, Style, Theme};
use plico_motion::{Easing, MotionArena, MotionId};
use std::{collections::HashMap, time::Duration};
use web_time::Instant;

struct Entry {
    id: MotionId,
    target: [f64; 42],
    shape: (bool, bool, bool, [u8; 26]),
    duration: u16,
}
pub(crate) struct StyleMotions {
    arena: MotionArena<42>,
    buttons: super::button_state::ButtonStates,
    entries: HashMap<Entity, Entry>,
    time: Option<Instant>,
    owners: HashMap<MotionId, Entity>,
    dirty: Vec<Entity>,
    sampled: Vec<MotionId>,
    enabled: bool,
}
impl Default for StyleMotions {
    fn default() -> Self {
        Self {
            arena: Default::default(),
            buttons: Default::default(),
            entries: Default::default(),
            time: None,
            owners: HashMap::new(),
            dirty: Vec::new(),
            sampled: Vec::new(),
            enabled: true,
        }
    }
}
impl StyleMotions {
    pub fn active(&self) -> bool {
        self.arena.is_active() || self.buttons.active()
    }
    pub fn advance(&mut self, now: Instant) -> bool {
        // Include the final exact sample even though active becomes false.
        let dirty = self.active();
        if let Some(previous) = self.time {
            self.buttons
                .advance(now.saturating_duration_since(previous), &mut self.dirty);
            self.arena
                .advance_into(now.saturating_duration_since(previous), &mut self.sampled);
            for id in &self.sampled {
                if let Some(entity) = self.owners.get(id) {
                    self.dirty.push(*entity);
                }
            }
        }
        self.time = Some(self.time.map_or(now, |previous| previous.max(now)));
        dirty || !self.dirty.is_empty()
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.dirty.extend(self.buttons.entities());
            self.buttons.clear();
            self.arena = Default::default();
            self.dirty.extend(self.entries.keys().copied());
            self.entries.clear();
            self.owners.clear();
        }
    }
    pub fn remove(&mut self, entity: Entity) {
        self.buttons.remove(entity);
        if let Some(entry) = self.entries.remove(&entity) {
            self.arena.remove(entry.id);
            self.owners.remove(&entry.id);
        }
    }
    pub fn drain_dirty(&mut self) -> std::vec::Drain<'_, Entity> {
        self.dirty.drain(..)
    }
    /// Theme reveal replaces the entire palette; do not crossfade its target again.
    pub fn snap_theme(&mut self) {
        self.buttons.clear();
        self.arena = Default::default();
        self.entries.clear();
        self.owners.clear();
    }
    pub fn remove_button(&mut self, entity: Entity) {
        self.buttons.remove(entity);
    }
    pub fn button_style(
        &mut self,
        entity: Entity,
        style: Style,
        theme: Theme,
        input: super::button_state::Interaction,
    ) -> (Style, f32) {
        self.buttons
            .style(entity, style, theme, input, self.enabled)
    }
    pub fn checked(&self, entity: Entity, target: bool) -> f32 {
        self.entries
            .get(&entity)
            .and_then(|e| self.arena.value(e.id))
            .map_or(u8::from(target) as f32, |v| v[9] as f32)
    }
    pub fn style(&mut self, entity: Entity, style: Style, theme: Theme, entering: bool) -> Style {
        if !self.enabled || style.transition_ms == 0 {
            self.remove(entity);
            return style;
        }
        let background = style.background.map_or(0, |color| theme.resolve(color));
        let base = [
            style.width.unwrap_or(0.) as f64,
            style.height.unwrap_or(0.) as f64,
            style.gap as f64,
            style.padding as f64,
            style.opacity as f64,
            style.radius as f64,
            ((background >> 16) & 255) as f64,
            ((background >> 8) & 255) as f64,
            (background & 255) as f64,
            u8::from(style.checked) as f64,
            style.horizontal_padding() as f64,
            style.vertical_padding() as f64,
            style.translate[0] as f64,
            style.translate[1] as f64,
            style.scale[0] as f64,
            style.scale[1] as f64,
        ];
        let extra = extra_lengths(style);
        let mut target = [0.; 42];
        target[..16].copy_from_slice(&base);
        for (index, value) in extra.iter().enumerate() {
            target[16 + index] = match value {
                Some(plico_core::Length::Px(v) | plico_core::Length::Percent(v)) => *v as f64,
                _ => 0.,
            };
        }
        let shape = (
            style.width.is_some(),
            style.height.is_some(),
            style.background.is_some(),
            extra.map(|v| match v {
                None => 0,
                Some(plico_core::Length::Auto) => 1,
                Some(plico_core::Length::Px(_)) => 2,
                Some(plico_core::Length::Percent(_)) => 3,
            }),
        );
        // Auto/explicit and absent/present paint changes snap. Do not invent a
        // pixel value for auto or an opaque RGB equivalent of no background.
        if self
            .entries
            .get(&entity)
            .is_some_and(|entry| entry.shape != shape)
        {
            self.remove(entity);
        }
        let entry = self.entries.entry(entity).or_insert_with(|| {
            let mut initial = target;
            if entering && style.modal_open && style.overlay {
                initial[4] = 0.;
            }
            Entry {
                id: self.arena.insert_tween(
                    initial,
                    target,
                    Duration::from_millis(if initial != target {
                        style.transition_ms as u64
                    } else {
                        0
                    }),
                    Easing::EaseInOut,
                ),
                target,
                shape,
                duration: style.transition_ms,
            }
        });
        self.owners.insert(entry.id, entity);
        if entry.target != target || entry.duration != style.transition_ms {
            self.arena.retarget_tween(
                entry.id,
                target,
                Duration::from_millis(style.transition_ms as u64),
                Easing::EaseInOut,
            );
            entry.target = target;
            entry.duration = style.transition_ms;
        }
        let value = self.arena.value(entry.id).expect("live style motion");
        let channel = |i: usize| value[i].round().clamp(0., 255.) as u32;
        let mut result = Style {
            width: style.width.map(|_| value[0] as f32),
            height: style.height.map(|_| value[1] as f32),
            gap: value[2] as f32,
            padding: value[3] as f32,
            padding_x: Some(value[10] as f32),
            padding_y: Some(value[11] as f32),
            opacity: value[4] as f32,
            translate: [value[12] as f32, value[13] as f32],
            scale: [value[14] as f32, value[15] as f32],
            radius: value[5] as f32,
            background: style
                .background
                .map(|_| Color::Literal((channel(6) << 16) | (channel(7) << 8) | channel(8))),
            ..style
        };
        apply_extra(&mut result, &value[16..]);
        result
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

fn extra_lengths(s: Style) -> [Option<plico_core::Length>; 26] {
    use plico_core::Length::Px;
    let l = s.layout;
    let edge = |v: Option<plico_core::Edges<plico_core::Length>>| {
        v.map_or([None; 4], |e| e.values().map(Some))
    };
    let mut slots = [None; 26];
    slots[..2].copy_from_slice(&l.min_size);
    slots[2..4].copy_from_slice(&l.max_size);
    slots[4..8].copy_from_slice(&edge(l.margin));
    slots[8..12].copy_from_slice(&edge(l.padding));
    slots[12..16].copy_from_slice(&edge(l.inset));
    slots[16..18].copy_from_slice(&l.gap);
    slots[18] = Some(l.flex_basis);
    slots[19] = l.aspect_ratio.map(Px);
    slots[20..24].copy_from_slice(&[
        s.min_width.map(Px),
        s.min_height.map(Px),
        s.max_width.map(Px),
        s.max_height.map(Px),
    ]);
    slots[24] = s.width_percent.map(plico_core::Length::Percent);
    slots[25] = s.height_percent.map(plico_core::Length::Percent);
    slots
}
fn apply_extra(s: &mut Style, values: &[f64]) {
    use plico_core::Length;
    let old = extra_lengths(*s);
    let slots: [Option<Length>; 26] = std::array::from_fn(|i| {
        old[i].map(|l| match l {
            Length::Px(_) => Length::Px(values[i] as f32),
            Length::Percent(_) => Length::Percent(values[i] as f32),
            Length::Auto => Length::Auto,
        })
    });
    s.layout.min_size.copy_from_slice(&slots[..2]);
    s.layout.max_size.copy_from_slice(&slots[2..4]);
    let edge = |start: usize| {
        plico_core::Edges::new(
            slots[start].unwrap(),
            slots[start + 1].unwrap(),
            slots[start + 2].unwrap(),
            slots[start + 3].unwrap(),
        )
    };
    if s.layout.margin.is_some() {
        s.layout.margin = Some(edge(4));
    }
    if s.layout.padding.is_some() {
        s.layout.padding = Some(edge(8));
    }
    if s.layout.inset.is_some() {
        s.layout.inset = Some(edge(12));
    }
    s.layout.gap.copy_from_slice(&slots[16..18]);
    let v = |i| values[i] as f32;
    s.layout.flex_basis = slots[18].unwrap();
    s.layout.aspect_ratio = s.layout.aspect_ratio.map(|_| v(19));
    s.min_width = s.min_width.map(|_| v(20));
    s.min_height = s.min_height.map(|_| v(21));
    s.max_width = s.max_width.map(|_| v(22));
    s.max_height = s.max_height.map(|_| v(23));
    s.width_percent = s.width_percent.map(|_| v(24));
    s.height_percent = s.height_percent.map(|_| v(25));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retarget_is_continuous_and_unmount_releases_motion() {
        let mut entities = plico_core::Arena::default();
        let e = entities.insert(());
        let mut motions = StyleMotions::default();
        let now = Instant::now();
        let a = Style {
            width: Some(100.),
            transition_ms: 200,
            ..Default::default()
        };
        let b = Style {
            width: Some(300.),
            ..a
        };
        motions.advance(now);
        assert_eq!(
            motions.style(e, a, Theme::default(), false).width,
            Some(100.)
        );
        assert_eq!(
            motions.style(e, b, Theme::default(), false).width,
            Some(100.)
        );
        motions.advance(now + Duration::from_millis(100));
        assert_eq!(
            motions.style(e, b, Theme::default(), false).width,
            Some(200.)
        );
        assert_eq!(
            motions.style(e, a, Theme::default(), false).width,
            Some(200.)
        );
        motions.advance(now + Duration::from_millis(300));
        assert_eq!(
            motions.style(e, a, Theme::default(), false).width,
            Some(100.)
        );
        assert!(!motions.active());
        motions.style(e, b, Theme::default(), false);
        motions.remove(e);
        assert_eq!(motions.len(), 0);
        assert!(!motions.active());
    }
    #[test]
    fn auto_dimensions_and_disabled_motion_snap_without_invented_sizes() {
        let mut entities = plico_core::Arena::default();
        let e = entities.insert(());
        let mut motions = StyleMotions::default();
        let a = Style {
            transition_ms: 200,
            ..Default::default()
        };
        motions.style(e, a, Theme::default(), false);
        let b = Style {
            width: Some(200.),
            ..a
        };
        assert_eq!(
            motions.style(e, b, Theme::default(), false).width,
            Some(200.)
        );
        let c = Style {
            width: Some(400.),
            ..b
        };
        motions.style(e, c, Theme::default(), false);
        assert!(motions.active());
        motions.set_enabled(false);
        assert_eq!(motions.len(), 0);
        assert_eq!(
            motions.style(e, c, Theme::default(), false).width,
            Some(400.)
        );
        assert!(!motions.active());
    }
}
