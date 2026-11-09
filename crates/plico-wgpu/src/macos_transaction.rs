//! Coupled native mode/resize policy; all layer mutation stays on AppKit's thread.
use crate::{
    Error, PresentationTick, Result,
    transaction_policy::{PassBoundary, TransactionPolicy},
};
use objc2::MainThreadMarker;
use objc2_app_kit::NSView;
use objc2_quartz_core::CATransaction;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::{Duration, Instant};
use winit::window::Window;

pub(crate) struct NativeTransaction {
    policy: TransactionPolicy,
    synced: Option<bool>,
    force_sync: bool,
    enabled: bool,
    tick: Option<PresentationTick>,
    interval: Duration,
    _main: MainThreadMarker,
}
impl NativeTransaction {
    pub fn new(surface: &wgpu::Surface<'_>) -> Option<Self> {
        let main = MainThreadMarker::new()?;
        // SAFETY: Read-only guard; no HAL resource is destroyed or borrowed
        // beyond the guard. Construction precedes background acquisition.
        let synced = unsafe { surface.as_hal::<wgpu::hal::api::Metal>() }?
            .render_layer()
            .lock()
            .presentsWithTransaction();
        Some(Self {
            policy: TransactionPolicy::default(),
            synced: Some(synced),
            force_sync: true,
            enabled: true,
            tick: None,
            interval: Duration::from_secs_f64(1. / 60.),
            _main: main,
        })
    }
    pub fn clock(&mut self, tick: Option<PresentationTick>, interval: Duration) {
        self.tick = tick;
        self.interval = interval;
    }
    pub fn defer(
        &mut self,
        window: &dyn Window,
        size: [u32; 2],
        pass_epoch: Option<PassBoundary>,
    ) -> bool {
        let Some(pass_epoch) = pass_epoch.filter(|_| self.enabled) else {
            self.enabled = false;
            self.policy.cancel();
            return false;
        };
        // Ordinary animation/input frames need no AppKit live-resize query.
        if !self.policy.requires_boundary(size, pass_epoch.epoch) {
            self.policy.cancel();
            return false;
        }
        let live = window.window_handle().ok().is_some_and(|handle| {
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return false;
            };
            // SAFETY: A live winit view, borrowed on the constructor's main thread.
            unsafe { handle.ns_view.cast::<NSView>().as_ref() }.inLiveResize()
        });
        self.policy.defer(
            size,
            live,
            self.tick,
            Instant::now(),
            self.interval,
            pass_epoch,
        )
    }
    pub fn sync(&self, fit: bool) -> bool {
        self.enabled && (fit || self.force_sync)
    }
    pub fn changes_mode(&self, sync: bool) -> bool {
        self.synced != Some(sync)
    }
    /// Caller has drained acquisition and disposed incompatible encoded work.
    pub fn apply(&mut self, surface: &wgpu::Surface<'_>, sync: bool) -> Result<()> {
        if !self.changes_mode(sync) {
            return Ok(());
        }
        // SAFETY: Only the layer property is mutated, on the main thread, with
        // no acquisition in flight. The guard never escapes or destroys a resource.
        let hal =
            unsafe { surface.as_hal::<wgpu::hal::api::Metal>() }.ok_or_else(|| Error::Surface {
                message: "Metal presentation layer unavailable".into(),
            })?;
        let layer = hal.render_layer().lock().clone();
        // Keep the layer retained, not the HAL mutex, across native commit.
        // Acquisition is drained and the window still owns this surface.
        drop(hal);
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        layer.setPresentsWithTransaction(sync);
        CATransaction::commit();
        self.synced = Some(sync);
        Ok(())
    }
    pub fn presented(&mut self, size: [u32; 2], sync: bool, pass_epoch: Option<PassBoundary>) {
        self.force_sync = false;
        self.policy
            .presented(size, sync, pass_epoch.map_or(0, |pass| pass.epoch));
    }
    pub fn reconfigured(&mut self) {
        self.force_sync = true;
        self.policy.cancel();
    }
    pub fn replaced(&mut self) {
        self.synced = None;
        self.reconfigured();
    }
    pub fn cancel(&mut self) {
        self.policy.cancel();
    }
    pub fn deferred(&self) -> bool {
        self.policy.deferred()
    }
    pub fn deadline(&self) -> Option<Instant> {
        if self.tick.is_none() {
            self.policy.deadline()
        } else {
            None
        }
    }
    pub fn stats(&self) -> [u64; 3] {
        self.policy.stats()
    }
}
