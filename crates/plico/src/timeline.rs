//! Coordinated scalar tweens on a shared playhead.
//!
//! Adapted from MyGO `ui/timeline.go` at revision
//! `f651b1061d917336f31c3ede523cb49c524172eb` (MIT). See
//! `licenses/MyGo-MIT.txt`.
use crate::{Easing, LocalState, UiContext};
use std::{
    cell::{Cell, RefCell},
    time::Duration,
};

/// A sampled phase in a repeating, discrete timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopPhase {
    /// Zero-based phase index in the supplied duration list.
    pub index: usize,
    /// Zero-based repetition of the entire list.
    pub cycle: u64,
}

/// A timed pulse sampled on the injected frame clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pulse {
    pub active: bool,
    /// True once when the hold expires. Cancelling does not report completion.
    pub finished: bool,
}

#[derive(Clone, Copy)]
struct PulseChannel {
    trigger: Option<u64>,
    deadline: Option<Duration>,
}

impl UiContext<'_> {
    /// Sample a repeating sequence of discrete phases. The first declaration
    /// starts at the given delay. Only phase boundaries request a new frame.
    /// Omitting the declaration resets the playhead, as with loop_progress.
    /// An empty list, an all-zero list or reduced motion disables the loop.
    pub fn loop_phases(
        &mut self,
        key: &'static str,
        delay: Duration,
        durations: &[Duration],
    ) -> Option<LoopPhase> {
        let total: u128 = durations.iter().map(Duration::as_nanos).sum();
        if total == 0 || self.display_preferences().reduce_motion {
            return None;
        }
        let now = self.now();
        let start = *self.channel_state(4, key, || now).read();
        let first = start.saturating_add(delay);
        if now < first {
            self.request_rebuild_at(first);
            return None;
        }
        let elapsed = now.saturating_sub(first).as_nanos();
        let mut within = elapsed % total;
        for (index, duration) in durations.iter().enumerate() {
            if within < duration.as_nanos() {
                let offset = Duration::new(
                    (within / 1_000_000_000) as u64,
                    (within % 1_000_000_000) as u32,
                );
                self.request_rebuild_at(now.saturating_add(*duration - offset));
                return Some(LoopPhase {
                    index,
                    cycle: (elapsed / total).min(u128::from(u64::MAX)) as u64,
                });
            }
            within -= duration.as_nanos();
        }
        None
    }

    /// Hold a visual signal when its trigger changes. Mounting with an initial
    /// trigger does not fire. Pass None to cancel or to arm the next trigger.
    /// Reduced motion cancels the hold. Only its deadline requests a frame.
    pub fn pulse(&mut self, key: &'static str, trigger: Option<u64>, hold: Duration) -> Pulse {
        let now = self.now();
        let reduced_motion = self.display_preferences().reduce_motion;
        let state = self.channel_state(5, key, || {
            Cell::new(PulseChannel {
                trigger,
                deadline: None,
            })
        });
        let state = state.read();
        let mut channel = state.get();
        if reduced_motion || trigger.is_none() {
            channel.deadline = None;
        } else if channel.trigger != trigger {
            channel.deadline = Some(now.saturating_add(hold));
        }
        channel.trigger = trigger;
        let result = if let Some(deadline) = channel.deadline {
            if now < deadline {
                self.request_rebuild_at(deadline);
                Pulse {
                    active: true,
                    finished: false,
                }
            } else {
                channel.deadline = None;
                Pulse {
                    active: false,
                    finished: true,
                }
            }
        } else {
            Pulse::default()
        };
        state.set(channel);
        result
    }
}

/// Repeat a timeline for as long as it remains declared.
pub const REPEAT_FOREVER: i32 = -1;

/// A signed offset used by relative timeline placements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineOffset(i128);

impl TimelineOffset {
    pub fn after(duration: Duration) -> Self {
        Self(duration.as_nanos().min(i128::MAX as u128) as i128)
    }

    /// A negative offset, used to overlap adjacent tweens.
    pub fn before(duration: Duration) -> Self {
        Self(-(duration.as_nanos().min(i128::MAX as u128) as i128))
    }
}

impl From<Duration> for TimelineOffset {
    fn from(value: Duration) -> Self {
        Self::after(value)
    }
}

/// Where a tween or label begins on a [`Timeline`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    kind: PlacementKind,
    offset: TimelineOffset,
    label: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlacementKind {
    AfterPrevious,
    WithPrevious,
    AtTime,
    AtLabel,
}

pub fn after_previous(offset: impl Into<TimelineOffset>) -> Placement {
    Placement {
        kind: PlacementKind::AfterPrevious,
        offset: offset.into(),
        label: None,
    }
}

pub fn with_previous(offset: impl Into<TimelineOffset>) -> Placement {
    Placement {
        kind: PlacementKind::WithPrevious,
        offset: offset.into(),
        label: None,
    }
}

pub fn at_time(time: Duration) -> Placement {
    Placement {
        kind: PlacementKind::AtTime,
        offset: time.into(),
        label: None,
    }
}

pub fn at_label(label: &'static str, offset: impl Into<TimelineOffset>) -> Placement {
    Placement {
        kind: PlacementKind::AtLabel,
        offset: offset.into(),
        label: Some(label),
    }
}

/// One scalar value moving between two endpoints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub duration: Duration,
    pub ease: Easing,
}

impl Tween {
    pub const fn new(from: f32, to: f32, duration: Duration) -> Self {
        Self {
            from,
            to,
            duration,
            ease: Easing::EaseOut,
        }
    }

    pub const fn easing(mut self, ease: Easing) -> Self {
        self.ease = ease;
        self
    }
}

/// Playback controls for a [`Timeline`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimelineOptions {
    /// Changing this value restarts playback.
    pub run: u64,
    /// Additional plays after the first, or [`REPEAT_FOREVER`].
    pub repeat: i32,
    pub alternate: bool,
    pub delay: Duration,
    pub repeat_delay: Duration,
    pub paused: bool,
    pub reverse: bool,
    pub seek: Option<Duration>,
    pub progress: Option<f32>,
    /// Continue opacity, blur and other non-spatial motion under reduced motion.
    pub non_motion: bool,
}

#[derive(Clone, Copy)]
struct Segment {
    start: Duration,
    duration: Duration,
    from: f32,
    to: f32,
    ease: Easing,
}

#[derive(Default)]
struct TimelineState {
    declared_epoch: u64,
    started: bool,
    built: bool,
    moving: bool,
    reverse: bool,
    holding: bool,
    run: u64,
    raw: Duration,
    duration: Duration,
    build: Duration,
    time: Duration,
    last_now: Duration,
    done: bool,
    over: bool,
    at_start: bool,
    rested_epoch: u64,
}

/// Several tweens sampled from one shared, controllable playhead.
pub struct Timeline<'u, 'e> {
    ui: &'u mut UiContext<'e>,
    state: LocalState<RefCell<TimelineState>>,
    previous_start: Duration,
    previous_end: Duration,
    labels: Vec<(&'static str, Duration)>,
}

impl<'u, 'e> Timeline<'u, 'e> {
    pub(crate) fn new(
        ui: &'u mut UiContext<'e>,
        key: &'static str,
        options: TimelineOptions,
    ) -> Self {
        assert!(
            options.repeat >= REPEAT_FOREVER,
            "a timeline repeat must be REPEAT_FOREVER or nonnegative"
        );
        if let Some(progress) = options.progress {
            assert!(progress.is_finite(), "timeline progress must be finite");
        }
        assert!(
            options.seek.is_none() || options.progress.is_none(),
            "a timeline takes seek or progress, not both"
        );
        let now = ui.now();
        let reduced_motion = ui.display_preferences().reduce_motion;
        let epoch = ui.engine.epoch;
        let state = ui.channel_state(3, key, || RefCell::new(TimelineState::default()));
        let mut request_frame = false;
        {
            let slot = state.read();
            let mut state = slot.borrow_mut();
            assert_ne!(
                state.declared_epoch, epoch,
                "a timeline key may be requested only once per declaration"
            );
            state.declared_epoch = epoch;
            state.duration = state.build;
            state.build = Duration::ZERO;
            tick(
                &mut state,
                options,
                now,
                reduced_motion,
                epoch,
                &mut request_frame,
            );
        }
        if request_frame {
            ui.request_animation_frame();
        }
        Self {
            ui,
            state,
            previous_start: Duration::ZERO,
            previous_end: Duration::ZERO,
            labels: Vec::new(),
        }
    }

    /// Add and sample a tween. With no placement it follows the previous one.
    pub fn tween(&mut self, tween: Tween) -> f32 {
        self.sample(tween, None)
    }

    /// Add and sample a tween at an explicit placement.
    pub fn tween_at(&mut self, tween: Tween, placement: Placement) -> f32 {
        self.sample(tween, Some(placement))
    }

    fn sample(&mut self, tween: Tween, placement: Option<Placement>) -> f32 {
        assert!(
            tween.from.is_finite() && tween.to.is_finite(),
            "timeline tween endpoints must be finite"
        );
        let start = self.place(placement);
        let segment = Segment {
            start,
            duration: tween.duration,
            from: tween.from,
            to: tween.to,
            ease: tween.ease,
        };
        let end = start.saturating_add(tween.duration);
        let mut request_frame = false;
        let value;
        {
            let slot = self.state.read();
            let mut state = slot.borrow_mut();
            if end > state.build {
                state.build = end;
                if state.done
                    && !state.over
                    && !state.reverse
                    && !state.holding
                    && end > state.duration
                {
                    state.moving = true;
                    request_frame = true;
                }
            }
            value = show(&state, segment);
        }
        if request_frame {
            self.ui.request_animation_frame();
        }
        self.previous_start = start;
        self.previous_end = end;
        value
    }

    /// Mark a named point, following the previous tween when placement is absent.
    pub fn label(&mut self, name: &'static str) {
        self.mark_label(name, None);
    }

    /// Mark a named point at an explicit placement.
    pub fn label_at(&mut self, name: &'static str, placement: Placement) {
        self.mark_label(name, Some(placement));
    }

    fn mark_label(&mut self, name: &'static str, placement: Option<Placement>) {
        let at = self.place(placement);
        self.labels.push((name, at));
    }

    pub fn duration(&self) -> Duration {
        let slot = self.state.read();
        let state = slot.borrow();
        state.build.max(state.duration)
    }

    pub fn time(&self) -> Duration {
        let slot = self.state.read();
        let state = slot.borrow();
        if state.over {
            return if state.at_start {
                Duration::ZERO
            } else {
                state.build.max(state.duration)
            };
        }
        state.time.min(state.build.max(state.duration))
    }

    pub fn progress(&self) -> f32 {
        let duration = self.duration();
        if duration.is_zero() {
            return 0.;
        }
        (self.time().as_secs_f64() / duration.as_secs_f64()).clamp(0., 1.) as f32
    }

    pub fn settled(&self) -> bool {
        let slot = self.state.read();
        let state = slot.borrow();
        state.done && (state.over || state.reverse || state.build <= state.duration)
    }

    pub fn arrived(&self) -> bool {
        let slot = self.state.read();
        slot.borrow().rested_epoch == self.ui.engine.epoch
    }

    fn place(&self, placement: Option<Placement>) -> Duration {
        let placement = placement.unwrap_or_else(|| after_previous(Duration::ZERO));
        let base = match placement.kind {
            PlacementKind::AfterPrevious => self.previous_end,
            PlacementKind::WithPrevious => self.previous_start,
            PlacementKind::AtTime => Duration::ZERO,
            PlacementKind::AtLabel => {
                let label = placement.label.expect("label placement has a label");
                self.labels
                    .iter()
                    .rev()
                    .find_map(|(name, at)| (*name == label).then_some(*at))
                    .unwrap_or_else(|| panic!("timeline has no label {label}"))
            }
        };
        add_offset(base, placement.offset)
    }
}

fn add_offset(base: Duration, offset: TimelineOffset) -> Duration {
    let magnitude = Duration::from_nanos(offset.0.unsigned_abs().min(u64::MAX as u128) as u64);
    if offset.0 < 0 {
        base.saturating_sub(magnitude)
    } else {
        base.saturating_add(magnitude)
    }
}

fn show(state: &TimelineState, segment: Segment) -> f32 {
    if state.over {
        return if state.at_start {
            segment.from
        } else {
            segment.to
        };
    }
    if segment.duration.is_zero() {
        return if state.time >= segment.start {
            segment.to
        } else {
            segment.from
        };
    }
    if state.time <= segment.start {
        return segment.from;
    }
    let end = segment.start.saturating_add(segment.duration);
    if state.time >= end {
        return segment.to;
    }
    let progress = (state.time - segment.start).as_secs_f64() / segment.duration.as_secs_f64();
    let eased = segment.ease.sample(progress) as f32;
    segment.from + (segment.to - segment.from) * eased
}

fn tick(
    state: &mut TimelineState,
    options: TimelineOptions,
    now: Duration,
    reduced_motion: bool,
    epoch: u64,
    request_frame: &mut bool,
) {
    if !state.started {
        state.started = true;
        state.last_now = now;
        state.run = options.run;
        cue(state, options, reduced_motion, epoch, request_frame);
        return;
    }
    state.built = true;
    let delta = now.saturating_sub(state.last_now);
    state.last_now = now;
    if !delta.is_zero() && state.moving {
        if state.reverse {
            state.raw = state
                .raw
                .saturating_sub(delta)
                .max(options.delay.min(state.raw));
        } else {
            state.raw = state.raw.saturating_add(delta);
        }
    }
    cue(state, options, reduced_motion, epoch, request_frame);
}

fn cue(
    state: &mut TimelineState,
    options: TimelineOptions,
    reduced_motion: bool,
    epoch: u64,
    request_frame: &mut bool,
) {
    if state.run != options.run {
        state.run = options.run;
        state.raw = Duration::ZERO;
        state.done = false;
        state.over = false;
        state.rested_epoch = 0;
        if options.reverse {
            state.raw = options
                .delay
                .saturating_add(total_duration(state.duration, options).unwrap_or(state.duration));
        }
    }
    state.reverse = options.reverse;
    let held = if let Some(seek) = options.seek {
        state.raw = options.delay.saturating_add(seek);
        true
    } else if let Some(progress) = options.progress {
        state.raw = options
            .delay
            .saturating_add(state.duration.mul_f32(progress.clamp(0., 1.)));
        true
    } else {
        false
    };
    if held {
        state.over = false;
    } else if !options.paused && !options.non_motion && reduced_motion {
        state.over = true;
    }
    if state.over {
        state.at_start =
            options.reverse || options.alternate && (options.repeat.saturating_add(1)) % 2 == 0;
        state.time = if state.at_start {
            Duration::ZERO
        } else {
            state.duration
        };
        state.done = true;
        state.moving = false;
        state.holding = true;
        if state.rested_epoch == 0 {
            state.rested_epoch = epoch;
        }
        return;
    }
    if let Some(end) = total_duration(state.duration, options) {
        state.raw = state.raw.min(options.delay.saturating_add(end));
    }
    let raw = state.raw.saturating_sub(options.delay);
    let (time, done) = playhead(
        raw,
        state.duration,
        options.repeat,
        options.alternate,
        options.repeat_delay,
    );
    state.time = time;
    state.done = if options.reverse { raw.is_zero() } else { done };
    if !raw.is_zero() && state.duration.is_zero() {
        state.done = state.built;
    }
    state.holding = options.paused || held;
    state.moving = !state.done && !state.holding;
    if state.done {
        if state.rested_epoch == 0 {
            state.rested_epoch = epoch;
        }
        return;
    }
    state.rested_epoch = 0;
    if state.moving || held && !state.built {
        *request_frame = true;
    }
}

fn total_duration(duration: Duration, options: TimelineOptions) -> Option<Duration> {
    if options.repeat == REPEAT_FOREVER || duration.is_zero() {
        return None;
    }
    let plays = options.repeat as u32 + 1;
    Some(
        duration
            .saturating_mul(plays)
            .saturating_add(options.repeat_delay.saturating_mul(plays - 1)),
    )
}

fn playhead(
    raw: Duration,
    duration: Duration,
    repeat: i32,
    alternate: bool,
    repeat_delay: Duration,
) -> (Duration, bool) {
    if raw.is_zero() || duration.is_zero() {
        return (raw, false);
    }
    let period = duration.saturating_add(repeat_delay).max(duration);
    let period_nanos = period.as_nanos();
    let iteration = raw.as_nanos() / period_nanos;
    let rest_nanos = raw.as_nanos() % period_nanos;
    let plays = repeat as i128 + 1;
    if repeat != REPEAT_FOREVER
        && (iteration as i128 >= plays
            || iteration as i128 == plays - 1 && rest_nanos >= duration.as_nanos())
    {
        return (
            if alternate && plays % 2 == 0 {
                Duration::ZERO
            } else {
                duration
            },
            true,
        );
    }
    let mut at = Duration::from_nanos(rest_nanos.min(u64::MAX as u128) as u64).min(duration);
    if alternate && iteration % 2 == 1 {
        at = duration.saturating_sub(at);
    }
    (at, false)
}
