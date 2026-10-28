//! Host-owned interaction weights; no business subscriptions or per-property tweens.
use plico::{Color, Entity, Style, Theme};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

#[derive(Clone, Copy, Default)]
pub(super) struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
}
impl Interaction {
    fn targets(self) -> [f32; 4] {
        if self.disabled {
            return [0., 0., 0., 1.];
        }
        [self.hovered, self.pressed, self.focused, false].map(|on| u8::from(on) as f32)
    }
}
struct Entry {
    value: [f32; 4],
    target: [f32; 4],
    from: [f32; 4],
    elapsed: Duration,
    duration: Duration,
}
#[derive(Default)]
pub(super) struct ButtonStates {
    entries: HashMap<Entity, Entry>,
    active: HashSet<Entity>,
}
impl ButtonStates {
    pub fn active(&self) -> bool {
        !self.active.is_empty()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.active.clear();
    }
    pub fn remove(&mut self, entity: Entity) {
        self.entries.remove(&entity);
        self.active.remove(&entity);
    }
    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entries.keys().copied()
    }
    pub fn advance(&mut self, dt: Duration, dirty: &mut Vec<Entity>) {
        let entries = &mut self.entries;
        self.active.retain(|entity| {
            let entry = entries.get_mut(entity).expect("active button");
            entry.elapsed = entry.elapsed.saturating_add(dt).min(entry.duration);
            let active = entry.elapsed < entry.duration;
            let t = if active {
                entry.elapsed.as_secs_f32() / entry.duration.as_secs_f32()
            } else {
                1.
            };
            let eased = 1. - (1. - t).powi(3);
            for ((value, from), target) in entry.value.iter_mut().zip(entry.from).zip(entry.target)
            {
                *value = if active {
                    from + (target - from) * eased
                } else {
                    target
                };
            }
            dirty.push(*entity); // Include the final exact sample.
            active
        });
    }
    pub fn style(
        &mut self,
        entity: Entity,
        mut style: Style,
        theme: Theme,
        input: Interaction,
        enabled: bool,
    ) -> (Style, f32) {
        let target = input.targets();
        let weights = if !enabled || style.transition_ms == 0 {
            self.remove(entity);
            target
        } else {
            let entry = self.entries.entry(entity).or_insert(Entry {
                value: target,
                target,
                from: target,
                elapsed: Duration::ZERO,
                duration: Duration::from_millis(style.transition_ms as u64),
            });
            let duration = Duration::from_millis(style.transition_ms as u64);
            if entry.target != target || entry.duration != duration {
                entry.from = entry.value;
                entry.target = target;
                entry.elapsed = Duration::ZERO;
                entry.duration = duration;
            }
            if entry.value != target {
                self.active.insert(entity);
            } else {
                self.active.remove(&entity);
            }
            entry.value
        };
        if let Some(base) = style.background {
            let inferred = match base {
                Color::Control => (Some(Color::ControlHover), Some(Color::ControlPressed)),
                Color::Accent => (Some(Color::AccentHover), Some(Color::AccentPressed)),
                _ => (None, None),
            };
            let rgb = |color| {
                let v = theme.resolve(color);
                [
                    (v >> 16 & 255) as f32,
                    (v >> 8 & 255) as f32,
                    (v & 255) as f32,
                ]
            };
            let mut color = rgb(base);
            for (over, weight) in [
                (style.hover_background.or(inferred.0), weights[0]),
                (style.pressed_background.or(inferred.1), weights[1]),
            ] {
                if let Some(over) = over {
                    for (channel, target) in color.iter_mut().zip(rgb(over)) {
                        *channel += (target - *channel) * weight;
                    }
                }
            }
            let [r, g, b] = color.map(|v| v.round().clamp(0., 255.) as u32);
            style.background = Some(Color::Literal((r << 16) | (g << 8) | b));
        }
        style.opacity *= 1. - 0.55 * weights[3];
        (style, weights[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (ButtonStates, Entity, Style) {
        let mut arena = plico::Arena::default();
        let entity = arena.insert(());
        (
            ButtonStates::default(),
            entity,
            Style {
                background: Some(Color::Literal(0x000000)),
                hover_background: Some(Color::Literal(0xffffff)),
                pressed_background: Some(Color::Literal(0xff0000)),
                transition_ms: 200,
                ..Default::default()
            },
        )
    }
    #[test]
    fn interruption_is_continuous_and_only_active_entities_are_sampled() {
        let (mut states, entity, style) = setup();
        let theme = Theme::default();
        states.style(entity, style, theme, Interaction::default(), true);
        let hover = Interaction {
            hovered: true,
            ..Default::default()
        };
        states.style(entity, style, theme, hover, true);
        let mut dirty = Vec::new();
        states.advance(Duration::from_millis(40), &mut dirty);
        assert_eq!(dirty, [entity]);
        let before = states.style(entity, style, theme, hover, true).0.background;
        let press = Interaction {
            hovered: true,
            pressed: true,
            ..Default::default()
        };
        assert_eq!(
            states.style(entity, style, theme, press, true).0.background,
            before
        );
        states.advance(Duration::from_secs(2), &mut dirty);
        assert_eq!(
            states.style(entity, style, theme, press, true).0.background,
            Some(Color::Literal(0xff0000))
        );
        assert!(!states.active());
        dirty.clear();
        states.advance(Duration::from_millis(16), &mut dirty);
        assert!(dirty.is_empty());
    }
    #[test]
    fn disabling_fades_to_045_and_suppresses_focus_and_press() {
        let (mut states, entity, style) = setup();
        let theme = Theme::default();
        states.style(
            entity,
            style,
            theme,
            Interaction {
                focused: true,
                pressed: true,
                ..Default::default()
            },
            true,
        );
        let disabled = Interaction {
            disabled: true,
            focused: true,
            pressed: true,
            ..Default::default()
        };
        states.style(entity, style, theme, disabled, true);
        states.advance(Duration::from_secs(2), &mut Vec::new());
        let (paint, focus) = states.style(entity, style, theme, disabled, true);
        assert_eq!(focus, 0.);
        assert!((paint.opacity - 0.45).abs() < 0.00001);
        assert_eq!(paint.background, Some(Color::Literal(0)));
    }
    #[test]
    fn reduced_motion_and_zero_duration_snap_and_release_entries() {
        for enabled in [true, false] {
            let (mut states, entity, mut style) = setup();
            states.style(
                entity,
                style,
                Theme::default(),
                Interaction::default(),
                true,
            );
            if enabled {
                style.transition_ms = 0;
            }
            let (paint, _) = states.style(
                entity,
                style,
                Theme::default(),
                Interaction {
                    hovered: true,
                    ..Default::default()
                },
                enabled,
            );
            assert_eq!(paint.background, Some(Color::Literal(0xffffff)));
            assert_eq!(states.entities().count(), 0);
            assert!(!states.active());
        }
    }
    #[test]
    fn unmount_and_theme_reset_discard_weights() {
        let (mut states, entity, style) = setup();
        let hover = Interaction {
            hovered: true,
            ..Default::default()
        };
        states.style(
            entity,
            style,
            Theme::default(),
            Interaction::default(),
            true,
        );
        states.style(entity, style, Theme::default(), hover, true);
        assert!(states.active());
        states.remove(entity);
        assert!(!states.active());
        assert_eq!(states.entities().count(), 0);
        states.style(entity, style, Theme::default(), hover, true);
        states.clear();
        assert_eq!(states.entities().count(), 0);
        assert_eq!(
            states
                .style(entity, style, Theme::default(), hover, true)
                .0
                .background,
            Some(Color::Literal(0xffffff))
        );
        assert!(!states.active());
    }
    #[test]
    fn sampling_is_step_independent_and_finishes_at_transition_deadline() {
        let (mut coarse, entity, style) = setup();
        let mut fine = ButtonStates::default();
        let theme = Theme::default();
        let hover = Interaction {
            hovered: true,
            ..Default::default()
        };
        for states in [&mut coarse, &mut fine] {
            states.style(entity, style, theme, Interaction::default(), true);
            states.style(entity, style, theme, hover, true);
        }
        coarse.advance(Duration::from_millis(100), &mut Vec::new());
        for _ in 0..10 {
            fine.advance(Duration::from_millis(10), &mut Vec::new());
        }
        assert_eq!(coarse.entries[&entity].value, fine.entries[&entity].value);
        assert!(coarse.active());
        coarse.advance(Duration::from_millis(100), &mut Vec::new());
        assert!(!coarse.active());
        assert_eq!(coarse.entries[&entity].value, hover.targets());
    }
    #[test]
    fn reversing_before_first_sample_does_not_leave_a_wakeup() {
        let (mut states, entity, style) = setup();
        let theme = Theme::default();
        states.style(entity, style, theme, Interaction::default(), true);
        states.style(
            entity,
            style,
            theme,
            Interaction {
                hovered: true,
                ..Default::default()
            },
            true,
        );
        assert!(states.active());
        states.style(entity, style, theme, Interaction::default(), true);
        assert!(!states.active());
    }
}
