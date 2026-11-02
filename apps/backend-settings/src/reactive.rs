//! Reactive workspace using the same widgets, layout and animation values as
//! the immediate comparison implementation.
use crate::{
    Page, activity_row,
    animation::{Animations, SIDEBAR_COMPACT, SIDEBAR_EXPANDED},
    color_icons, icons, progress_bar,
};
use plico::{
    Changes, Engine, Entity, FrameError, FrameTime, Router, TextBuffer, Theme, View, prelude::*,
};
use plico_runtime::{
    HostView,
    regions::{
        Access, OnChange, OnClick, OnClickWith, Reactive, RegionUi, Regions, ReplayBatch, State,
        outlet_fill,
    },
};
use std::time::Duration;

struct Workspace {
    selected_project: u64,
    reverse: bool,
}
struct Editor {
    name: TextBuffer,
    saved: String,
    saves: usize,
}
impl Default for Editor {
    fn default() -> Self {
        let name = TextBuffer::new();
        name.set("Plico workspace".to_owned().into());
        Self {
            name,
            saved: String::new(),
            saves: 0,
        }
    }
}
struct Options {
    notifications: bool,
    advanced: bool,
    reduced_motion: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            notifications: true,
            advanced: false,
            reduced_motion: false,
        }
    }
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            selected_project: 1,
            reverse: false,
        }
    }
}

#[derive(Clone, Copy)]
struct RootStates {
    router: State<Router<Page>>,
    theme: State<Theme>,
    animations: State<Animations>,
    compact: State<bool>,
    inspector: State<bool>,
    options: State<Options>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AnimationPolicy {
    now: Duration,
    page: Page,
    compact: bool,
    inspector: bool,
    reduced: bool,
    narrow: bool,
    details_visible: bool,
}

/// Complete state-driven workspace. The explicit frame policy runs before both
/// local replay and full declaration, so animation never depends on the root
/// render function happening to execute.
pub struct Settings {
    pub regions: Regions,
    states: RootStates,
    last_policy: Option<AnimationPolicy>,
    frame_time: FrameTime,
    last_width: f32,
}
impl Default for Settings {
    fn default() -> Self {
        let mut states = None;
        let regions = Regions::new(|app| {
            let current = RootStates {
                router: app.state(Router::default),
                theme: app.state(Theme::light),
                animations: app.state(Animations::default),
                compact: app.state(|| false),
                inspector: app.state(|| false),
                options: app.state(Options::default),
            };
            app.theme_state(current.theme);
            app.root(
                (
                    current.router,
                    current.theme,
                    current.animations,
                    current.compact,
                    current.inspector,
                    current.options,
                ),
                root,
            );
            states = Some(current);
        });
        Self {
            regions,
            states: states.unwrap(),
            last_policy: None,
            frame_time: FrameTime::default(),
            last_width: 0.,
        }
    }
}
impl Settings {
    fn policy(&self, now: Duration, width: f32) -> AnimationPolicy {
        let page = self.regions.access(self.states.router).read().current();
        let compact = *self.regions.access(self.states.compact).read();
        let inspector = *self.regions.access(self.states.inspector).read();
        let reduced = self
            .regions
            .access(self.states.options)
            .read()
            .reduced_motion;
        AnimationPolicy {
            now,
            page,
            compact,
            inspector,
            reduced,
            narrow: width < 760.,
            details_visible: width >= 1000.,
        }
    }
    fn tick(&mut self, now: Duration, width: f32) {
        let policy = self.policy(now, width);
        if self.last_policy == Some(policy) {
            return;
        }
        self.last_policy = Some(policy);
        self.last_width = width;
        self.regions
            .access(self.states.animations)
            .update_if(|animations| {
                let before = (
                    animations.sidebar(),
                    animations.inspector(),
                    animations.progress(),
                    animations.active(),
                );
                animations.advance(now);
                animations.set_reduced(policy.reduced);
                if policy.page != Page::Overview {
                    animations.cancel();
                }
                animations.targets(
                    policy.compact || policy.narrow,
                    policy.inspector && policy.page == Page::Overview && policy.details_visible,
                );
                before
                    != (
                        animations.sidebar(),
                        animations.inspector(),
                        animations.progress(),
                        animations.active(),
                    )
            });
    }
    fn policy_targets_changed(&self, now: Duration, width: f32) -> bool {
        let current = self.policy(now, width);
        self.last_policy.is_none_or(|previous| {
            AnimationPolicy {
                now: previous.now,
                ..current
            } != previous
        })
    }
    fn animation_requested(&self, now: Duration, width: f32) -> bool {
        self.regions.access(self.states.animations).read().active()
            || self.policy_targets_changed(now, width)
    }
    fn show_with_tick(&mut self, cx: &mut UiContext<'_>) {
        let now = cx.now();
        let width = cx.viewport_width();
        self.tick(now, width);
        HostView::show(&mut self.regions, cx);
        if self.animation_requested(now, width) {
            cx.request_animation_frame();
        }
    }

    pub fn current_page(&self) -> Page {
        self.regions.access(self.states.router).read().current()
    }
    pub fn go(&self, page: Page) {
        self.regions
            .access(self.states.router)
            .update(|router| router.go(page));
    }
    pub fn set_dark(&self, dark: bool) {
        self.regions.access(self.states.theme).set(if dark {
            Theme::dark()
        } else {
            Theme::light()
        });
    }
    pub fn set_advanced(&self, advanced: bool) {
        self.regions
            .access(self.states.options)
            .update(|options| options.advanced = advanced);
    }
    pub fn set_inspector(&self, inspector: bool) {
        self.regions.access(self.states.inspector).set(inspector);
    }
    pub fn set_reduced_motion(&self, reduced: bool) {
        self.regions
            .access(self.states.options)
            .update(|options| options.reduced_motion = reduced);
    }
    pub fn set_compact(&self, compact: bool) {
        self.regions.access(self.states.compact).set(compact);
    }
    pub fn compact(&self) -> bool {
        *self.regions.access(self.states.compact).read()
    }
    pub fn progress(&self) -> f32 {
        self.regions
            .access(self.states.animations)
            .read()
            .progress()
    }
    pub fn sidebar_width(&self) -> f32 {
        self.regions.access(self.states.animations).read().sidebar()
    }
    pub fn inspector_width(&self) -> f32 {
        self.regions
            .access(self.states.animations)
            .read()
            .inspector()
    }
    pub fn animation_active(&self) -> bool {
        self.regions.access(self.states.animations).read().active()
    }
}
impl View for Settings {
    fn theme(&self) -> Theme {
        HostView::theme(&self.regions)
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.show_with_tick(cx);
    }
}
impl HostView for Settings {
    fn theme(&self) -> Theme {
        HostView::theme(&self.regions)
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.show_with_tick(cx);
    }
    fn scene_cache_roots(&self) -> Vec<Entity> {
        self.regions.scene_cache_roots()
    }
    fn replay_regions(&mut self, engine: &mut Engine) -> Option<Result<ReplayBatch, FrameError>> {
        let now = engine.time().now;
        let width = engine.viewport_width();
        self.tick(now, width);
        let mut result = self.regions.replay_regions(engine);
        // Event actions run after the first tick. Like an immediate settle pass,
        // apply their new targets at the same timestamp before advancing time.
        for _ in 0..8 {
            if !matches!(result, Some(Ok(_))) || !self.policy_targets_changed(now, width) {
                break;
            }
            self.tick(now, width);
            let next = self.regions.replay_regions(engine);
            match (result.as_mut(), next) {
                (Some(Ok(batch)), Some(Ok(next))) => merge_batch(batch, next),
                (_, Some(next)) => {
                    result = Some(next);
                    break;
                }
                (_, None) => break,
            }
        }
        if self.animation_requested(now, width)
            && let Some(Ok(batch)) = &mut result
        {
            batch.changes.insert(Changes::ANIMATE);
        }
        result
    }
    fn set_wake_callback(&mut self, wake: plico_renderer::WakeCallback) {
        self.regions.set_wake_callback(wake);
    }
    fn poll_reload(&mut self) -> bool {
        self.regions.poll_reload()
    }
    fn set_frame_time(&mut self, time: FrameTime) {
        self.frame_time = time;
    }
    fn snapshot_changed(&self) -> bool {
        self.regions.snapshot_changed()
            || self.policy_targets_changed(self.frame_time.now, self.last_width)
            || self.last_policy.is_some_and(|previous| {
                previous.now != self.frame_time.now
                    && self.regions.access(self.states.animations).read().active()
            })
    }
}

fn merge_batch(batch: &mut ReplayBatch, next: ReplayBatch) {
    batch.stats.visited += next.stats.visited;
    batch.stats.created += next.stats.created;
    batch.stats.reused += next.stats.reused;
    batch.stats.removed += next.stats.removed;
    batch.stats.text_writes += next.stats.text_writes;
    batch.stats.paint_updates += next.stats.paint_updates;
    batch.stats.order_changes += next.stats.order_changes;
    batch.dirty.extend(next.dirty);
    batch.changes |= next.changes;
    batch.passes += next.passes;
}

type RootAccess<'a> = (
    Access<'a, Router<Page>>,
    Access<'a, Theme>,
    Access<'a, Animations>,
    Access<'a, bool>,
    Access<'a, bool>,
    Access<'a, Options>,
);
#[component]
#[rustfmt::skip]
fn root(cx: &mut UiContext, states: RootAccess<'_>) {
    let (router, theme, animations, compact, inspector, options) = states;
    let workspace = cx.local_state(Workspace::default);
    let editor = cx.local_state(Editor::default);
    row(cx).width_percent(100.).height_percent(100.).align(Align::Stretch).background(Color::Surface) + |cx| {
        cx.region((router.handle(), animations.handle(), compact.handle()), sidebar);
        column(cx).width(0.).grow(1.).min_width(0.).padding(20).gap(18) + |cx| {
            cx.region((router.handle(), theme.handle()), toolbar);
            cx.region((router.handle(), inspector.handle()), heading);
            outlet_fill(
                cx,
                router.handle(),
                (
                    animations.handle(),
                    inspector.handle(),
                    options.handle(),
                    workspace,
                    editor,
                    theme.handle(),
                ),
                page,
            );
            row(cx).gap(8) + |cx| {
                text(cx, "Shared Plico view · Taffy geometry").font_size(12).color(Color::Muted).grow(1.);
                text_empty(cx).font_size(12).color(Color::Muted).rx(
                    (animations.handle(),),
                    |node, (animations,)| {
                        node.text(if animations.read().active() {
                            "Animating"
                        } else {
                            "Idle · event driven"
                        });
                    },
                );
            };
        };
    };
}

#[component]
#[rustfmt::skip]
fn toolbar(cx: &mut UiContext, (router, theme): (Access<'_, Router<Page>>, Access<'_, Theme>)) {
    row(cx).gap(10).wrap(true) + |cx| {
        text(cx, "PLICO / WORKSPACE").font_size(12).color(Color::Muted).grow(1.);
        button(cx).icon(&icons::ARROW_LEFT).aria_label("Back").disabled(!router.read().can_back())
            .on_click((router.handle(),), |(router,)| {
            router.update(|router| router.back());
        });
        button(cx).icon(&icons::ARROW_RIGHT).aria_label("Forward").disabled(!router.read().can_forward())
            .on_click((router.handle(),), |(router,)| {
            router.update(|router| router.forward());
        });
        let mut dark = *theme.read() == Theme::dark();
        checkbox(cx, "Dark theme", &mut dark).on_change((theme.handle(),), |(theme,), dark| {
            theme.set(if dark { Theme::dark() } else { Theme::light() });
        });
    };
}

#[component]
#[rustfmt::skip]
fn heading(cx: &mut UiContext, (router, inspector): (Access<'_, Router<Page>>, Access<'_, bool>)) {
    row(cx).gap(12).wrap(true) + |cx| {
        column(cx).width(0.).grow(1.).min_width(120.).gap(6) + |cx| {
            text(cx, router.read().current().label()).font_size(28).weight(700);
            text(cx, match router.read().current() {
                Page::Overview => "A native workspace. Flex, Grid and retained controls working together.",
                Page::Activity => "A keyed list, a fixed header and a native scroll viewport.",
                Page::Preferences => "Application state survives navigation. The editor remains a native control.",
            }).font_size(13).color(Color::Muted);
        };
        if router.read().current() == Page::Overview {
            button(cx).icon(&icons::PANEL_RIGHT).text("Details").on_click((inspector.handle(),), |(inspector,)| {
                inspector.update(|visible| *visible = !*visible);
            });
        }
    };
}

#[component]
#[rustfmt::skip]
fn sidebar(
    cx: &mut UiContext,
    (router, animations, compact): (
        Access<'_, Router<Page>>,
        Access<'_, Animations>,
        Access<'_, bool>,
    ),
) {
    let width = animations.read().sidebar();
    let expand = ((f64::from(width) - SIDEBAR_COMPACT) / (SIDEBAR_EXPANDED - SIDEBAR_COMPACT)).clamp(0., 1.) as f32;
    let collapsed = *compact.read() || cx.viewport_width() < 760.;
    column(cx).width(width).height_percent(100.).shrink(0.).padding(12).gap(18).clip(true).background(Color::Panel)
    + |cx|
    {
        row(cx).gap(8).height(38.) + |cx| {
            svg(cx, &color_icons::APPS).size(26., 26.);
            text(cx, "plico").font_size(23).weight(700).opacity(expand).ellipsis(true);
        };
        column(cx).gap(8) + |cx| {
            for page in Page::ALL {
                nav_entry(cx, &router, *page, expand);
            }
        };
        column(cx).height(0.).grow(1.).end();
        let forced_narrow = cx.viewport_width() < 760.;
        button(cx).icon(if collapsed { &icons::PANEL_LEFT_OPEN } else { &icons::PANEL_LEFT_CLOSE })
            .aria_label("Toggle sidebar").height(36.).width_percent(100.).disabled(forced_narrow)
            .on_click((compact.handle(),), |(compact,)| {
            compact.update(|compact| *compact = !*compact);
        });
        text(cx, "Backend playground").font_size(12).color(Color::Muted).ellipsis(true).opacity(expand);
    };
}

#[component]
#[rustfmt::skip]
fn nav_entry(cx: &mut UiContext, router: &Access<'_, Router<Page>>, page: Page, expand: f32) {
    cx.keyed(page.id(), |cx| {
        let selected = router.read().show(page);
        let m = cx.theme().metrics;
        button_base(cx, page.label()).width_percent(100.).padding_x(m.space(2)).padding_y(m.space(1))
            .height(m.control_height as f32 + 4.).radius(m.radius as f32)
            .background(if selected { Color::Selection } else { Color::Panel }).hover_background(Color::ControlHover)
            .pressed_background(Color::ControlPressed).color(if selected { Color::Accent } else { Color::Muted })
            .selected(selected).transition(m.interaction_ms).gap(m.spacing).children(|cx| {
            if let Some(icon) = page.icon() {
                svg(cx, icon).size(16., 16.);
            }
            text(cx, page.label()).weight(if selected { 600 } else { 400 }).font_size(m.font_size).ellipsis(true)
                .opacity(expand);
        }).response.on_click_with((router.handle(),), page, |(router,), page| {
            router.update(|router| router.go(page));
        });
    });
}

type PageAccess<'a> = (
    Access<'a, Animations>,
    Access<'a, bool>,
    Access<'a, Options>,
    Access<'a, Workspace>,
    Access<'a, Editor>,
    Access<'a, Theme>,
);

#[component]
#[rustfmt::skip]
fn page(
    cx: &mut UiContext,
    page: Page,
    (animations, inspector, options, workspace, editor, theme): PageAccess<'_>,
) {
    match page {
        Page::Overview => cx.region_fill((animations.handle(), inspector.handle(), workspace.handle()), overview),
        Page::Activity => cx.region_fill((animations.handle(), workspace.handle()), activity),
        Page::Preferences => preferences(cx, (options, editor, theme)),
    }
}

#[component]
#[rustfmt::skip]
fn overview(
    cx: &mut UiContext,
    (animations, inspector, workspace): (
        Access<'_, Animations>,
        Access<'_, bool>,
        Access<'_, Workspace>,
    ),
) {
    let content = cx.viewport_width() - animations.read().sidebar() - 40.;
    column(cx).height(0.).grow(1.).scroll_y(true).gap(18) + |cx| {
        let tracks = if content >= 660. {
            3
        } else if content >= 400. { 2 } else { 1 };
        grid(cx).grid_columns(std::iter::repeat_n(Track::Fr(1.), tracks)).gap(12) + |cx| {
            stat(cx, "PROJECTS", "12", Some("4 workspaces active"));
            stat(cx, "STORAGE", "2.4 GB", Some("Local demo data"));
            stat(cx, "SYNC HEALTH", "98.6%", Some("Simulated workload"));
        };
        row(cx).gap(16).align(Align::Stretch).min_height(286.) + |cx| {
            column(cx).width(0.).grow(1.).min_width(0.).padding(18).gap(14).background(Color::Panel)
                .border(1., Color::Border).radius(8.) + |cx|
            {
                text(cx, "Projects").font_size(18).weight(650);
                text(cx, "Select a project; open Details to test the animated split pane.").font_size(13).color(Color::Muted);
                for (id, title, note) in [
                    (1u64, "Design system", "Components & typography"),
                    (2, "Desktop client", "Native backend integration"),
                    (3, "Embedded panel", "A smaller rendering target"),
                ] {
                    cx.keyed(id, |cx| {
                        row(cx).gap(10).padding_y(8) + |cx| {
                            icon(cx,
                                if id == 3 { &color_icons::PUZZLE_PIECE } else { &color_icons::APPS });
                            column(cx).width(0.).grow(1.).gap(4) + |cx| {
                                text(cx, title).weight(600);
                                text(cx, note).font_size(12).color(Color::Muted);
                            };
                            button(cx).text(if workspace.read().selected_project == id {
                                "Selected"
                            } else {
                                "Open"
                            }).on_click_with(
                                (workspace.handle(), inspector.handle()),
                                id,
                                |(workspace, inspector), id| {
                                    workspace.update(|workspace| {
                                        workspace.selected_project = id
                                    });
                                    inspector.set(true);
                                },
                            );
                        };
                    });
                }
            };
            if animations.read().inspector() > 0.
                || *inspector.read() && cx.viewport_width() >= 1000.
            {
                column(cx).width(animations.read().inspector()).shrink(0.).clip(true).background(Color::Panel)
                    .radius(8.) + |cx|
                {
                    column(cx).width_percent(100.).padding(18).gap(14) + |cx| {
                        text(cx, "Project details").font_size(18).weight(650);
                        text!(
                            cx,
                            "Workspace 0{}",
                            workspace.read().selected_project
                        ).weight(600);
                        text(cx, "The pane width is sampled from the same injected clock on both backends.")
                            .font_size(13).color(Color::Muted);
                        text(cx, "Status").weight(600);
                        text(cx, "Ready to sync").color(Color::Accent);
                        button(cx).text("Close details")
                            .on_click((inspector.handle(),), |(inspector,)| {
                            inspector.set(false)
                        });
                    };
                };
            }
        };
        column(cx).padding(18).gap(12).background(Color::Panel).border(1., Color::Border).radius(8.) + |cx| {
            row(cx).gap(10).wrap(true) + |cx| {
                text(cx, "Sync preview").font_size(18).weight(650).grow(1.);
                button(cx).icon(&icons::PLAY).text("Run preview").primary()
                    .on_click((animations.handle(),), |(animations,)| {
                    animations.update(|animations| animations.start());
                });
                button(cx).icon(&icons::PAUSE).text("Stop").disabled(!animations.read().progress_running())
                    .on_click((animations.handle(),), |(animations,)| {
                    animations.update(|animations| animations.cancel())
                });
            };
            text(cx, "A finite, simulated job. No network traffic.").font_size(13).color(Color::Muted);
            progress_bar(cx, animations.read().progress());
            text!(
                cx,
                "{}% complete",
                (animations.read().progress() * 100.).round() as u32
            ).font_size(13).color(Color::Muted);
        };
    };
}

#[component]
#[rustfmt::skip]
fn activity(
    cx: &mut UiContext,
    (animations, workspace): (Access<'_, Animations>, Access<'_, Workspace>),
) {
    column(cx).height(0.).grow(1.).gap(18) + |cx| {
        row(cx).gap(12) + |cx| {
            text(cx, "30 local events").color(Color::Muted).grow(1.);
            button(cx).text("Reverse order").on_click(
                (workspace.handle(),),
                |(workspace,)| {
                    workspace.update(|workspace| workspace.reverse = !workspace.reverse);
                },
            );
        };
        let available =
            (cx.viewport_width() - animations.read().sidebar() - 40. - 24. - 24. - 20. - 84.).max(0.);
        let tracks = [
            Track::Px(available * 0.58),
            Track::Px(available * 0.42),
            Track::Px(84.),
        ];
        activity_row(cx, "EVENT", "WORKSPACE", "STATE", true, tracks);
        column(cx).height(0.).grow(1.).scroll_y(true).gap(4) + |cx| {
            cx.each_gap(
                4.,
                (animations.handle(), workspace.handle()),
                |(_, workspace)| {
                    (0..30u64).map(|index| {
                        if workspace.read().reverse { 29 - index } else { index }
                    }).collect()
                },
                activity_item,
            );
        };
    };
}

fn activity_item(
    cx: &mut UiContext,
    (animations, _): (Access<'_, Animations>, Access<'_, Workspace>),
    id: u64,
) {
    let available =
        (cx.viewport_width() - animations.read().sidebar() - 40. - 24. - 24. - 20. - 84.).max(0.);
    let tracks = [
        Track::Px(available * 0.58),
        Track::Px(available * 0.42),
        Track::Px(84.),
    ];
    activity_row(
        cx,
        format_args!("Sync checkpoint {:02}", id + 1),
        if id.is_multiple_of(2) {
            "Desktop client"
        } else {
            "Design system"
        },
        if id.is_multiple_of(5) {
            "Queued"
        } else {
            "Complete"
        },
        false,
        tracks,
    );
}

#[component]
#[rustfmt::skip]
fn preferences(
    cx: &mut UiContext,
    (options, editor, theme): (Access<'_, Options>, Access<'_, Editor>, Access<'_, Theme>),
) {
    column(cx).height(0.).grow(1.).scroll_y(true).gap(18) + |cx| {
        cx.region((editor.handle(), options.handle()), profile);
        cx.region((theme.handle(),), |cx, (theme,)| {
            row(cx).gap(12).wrap(true) + |cx| {
                let mut dark = *theme.read() == Theme::dark();
                checkbox(cx, "Dark theme", &mut dark).on_change(
                    (theme.handle(),),
                    |(theme,), dark| {
                        theme.set(if dark { Theme::dark() } else { Theme::light() });
                    },
                );
                button(cx).text("Disabled action").disabled(true);
            };
        });
        cx.component((editor.handle(),), save_actions);
        text(cx, "No changes saved yet").color(Color::Muted).rx(
            (editor.handle(),),
            |node, (editor,)| {
                let editor = editor.read();
                if !editor.saved.is_empty() {
                    node.text(editor.saved.clone());
                }
            },
        );
        cx.region((options.handle(),), |cx, (options,)| {
            let mut reduced = options.read().reduced_motion;
            checkbox(cx, "Reduce motion", &mut reduced).on_change(
                (options.handle(),),
                |(options,), reduced| {
                    options.update(|options| options.reduced_motion = reduced);
                },
            );
        });
    };
}

#[component]
#[rustfmt::skip]
fn profile(cx: &mut UiContext, (editor, options): (Access<'_, Editor>, Access<'_, Options>)) {
    column(cx).padding(18).gap(14).background(Color::Panel).border(1., Color::Border).radius(8.) + |cx| {
        text(cx, "Profile / 工作区").font_size(18).weight(600);
        text(cx, "Workspace name");
        text_input(cx, "Workspace name", &editor.read().name).placeholder("Enter a name").width_percent(100.)
            .height(36.).on_change((editor.handle(),), |(editor,), _| editor.notify());
        text!(
            cx,
            "Current name: {}",
            editor.read().name.text().unwrap_or_default()
        ).color(Color::Muted);
        let mut notifications = options.read().notifications;
        checkbox(cx, "Enable notifications", &mut notifications).on_change(
            (options.handle(),),
            |(options,), value| {
                options.update(|options| options.notifications = value);
            },
        );
        let mut advanced = options.read().advanced;
        checkbox(cx, "Show advanced options", &mut advanced).on_change(
            (options.handle(),),
            |(options,), value| {
                options.update(|options| options.advanced = value);
            },
        );
        cx.when(options.handle(), |options| options.read().advanced, advanced_options);
    };
}

#[component]
#[rustfmt::skip]
fn advanced_options(cx: &mut UiContext, _options: Access<'_, Options>) {
    column(cx).gap(8).padding(12).background(Color::Control) + |cx| {
        text(cx, "Advanced options").weight(600);
        text(cx, "This branch is mounted on demand. Toggle it to test removal and recreation.");
        row(cx).gap(8) + |cx| {
            button(cx).text("Nested action").disabled(true);
            button(cx).text("Another action").disabled(true);
        };
    };
}

#[component]
#[rustfmt::skip]
fn save_actions(cx: &mut UiContext, (editor,): (Access<'_, Editor>,)) {
    row(cx).gap(10).wrap(true) + |cx| {
        button(cx).icon(&icons::CHECK).text("Save changes").primary().on_click((editor.handle(),), |(editor,)| {
            editor.update(|editor| {
                editor.saves += 1;
                editor.saved = format!(
                    "Saved {}: {}",
                    editor.saves,
                    editor.name.text().unwrap_or_default()
                );
            });
        });
        button(cx).text("Reset name").on_click((editor.handle(),), |(editor,)| {
            editor.read().name.set("Plico workspace".to_owned().into());
            editor.notify();
        });
    };
}
