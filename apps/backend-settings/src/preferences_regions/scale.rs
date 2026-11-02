//! Fully mounted copies of the real form, for CPU scaling experiments.
use super::{frame, preferences};
use crate::ImmediateSettings;
use plico::{Theme, prelude::*};
use plico_runtime::{
    HostView,
    regions::{RegionUi, Regions},
};

pub struct ImmediatePanels {
    panels: Vec<ImmediateSettings>,
    dark: bool,
    reduced: bool,
}
impl ImmediatePanels {
    pub fn new(count: usize) -> Self {
        Self {
            panels: (0..count).map(|_| ImmediateSettings::default()).collect(),
            dark: false,
            reduced: false,
        }
    }
}
impl HostView for ImmediatePanels {
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        frame(cx, |cx| {
            for (index, panel) in self.panels.iter_mut().enumerate() {
                cx.keyed(index as u64, |cx| {
                    panel.dark = self.dark;
                    panel.reduced_motion = self.reduced;
                    column(cx).gap(18).children(|cx| panel.preferences(cx));
                    self.dark = panel.dark;
                    self.reduced = panel.reduced_motion;
                });
            }
        });
    }
}

pub fn region_panels(count: usize) -> Regions {
    Regions::new(|app| {
        let count = app.state(|| count);
        let theme = app.persisted_state(1, Theme::light);
        let reduced = app.persisted_state(2, || false);
        app.theme_state(theme);
        app.root((count, theme, reduced), |cx, (count, theme, reduced)| {
            frame(cx, |cx| {
                for index in 0..*count.read() {
                    cx.component_keyed(
                        index as u64,
                        (theme.handle(), reduced.handle()),
                        preferences,
                    );
                }
            });
        });
    })
}

/// Diagnostic control: identical region API, state and layout topology, but no
/// local replay hook. This still pays region bookkeeping during a full show;
/// it is not a replacement for the ordinary immediate baseline above.
pub struct FullRegionPanels(pub Regions);
impl FullRegionPanels {
    pub fn new(count: usize) -> Self {
        Self(region_panels(count))
    }
}
impl HostView for FullRegionPanels {
    fn theme(&self) -> Theme {
        self.0.theme()
    }
    fn snapshot_changed(&self) -> bool {
        self.0.snapshot_changed()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.0.show(cx);
    }
}
