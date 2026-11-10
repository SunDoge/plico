//! Optional GPU frame timing with timestamp queries.
//!
//! A frame's root command encoder is bracketed by two empty compute passes that
//! write timestamp query indices 0 and 1. The results are resolved and copied
//! into a readback buffer at the end of the encoder, which is mapped only after
//! submission. Slots are reused, so a frame never waits for GPU completion.

use super::{FrameBudget, Renderer};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const SLOTS: usize = 4;
const MAP_PENDING: u8 = 0;
const MAP_READY: u8 = 1;
const MAP_FAILED: u8 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotState {
    Free,
    Recording,
    Pending,
}

struct Slot {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// `MAP_PENDING`, `MAP_READY` or `MAP_FAILED`, updated by the map callback.
    mapped: Arc<AtomicU8>,
    state: SlotState,
}

/// Bracket one command encoder at a time, without stalling the CPU.
pub(super) struct GpuTimer {
    slots: Vec<Slot>,
    next: usize,
    last_ms: Option<f64>,
}

impl GpuTimer {
    pub(super) fn new(device: &wgpu::Device) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let slots = (0..SLOTS)
            .map(|_| Slot {
                queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Plico frame budget"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Plico frame budget resolve"),
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Plico frame budget readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                mapped: Arc::new(AtomicU8::new(MAP_PENDING)),
                state: SlotState::Free,
            })
            .collect();
        Some(Self {
            slots,
            next: 0,
            last_ms: None,
        })
    }

    /// Most recently completed frame's GPU time, in milliseconds.
    pub(super) fn last_ms(&self) -> Option<f64> {
        self.last_ms
    }

    /// Reserve a slot and write the opening timestamp into `encoder`.
    pub(super) fn begin(&mut self, encoder: &mut wgpu::CommandEncoder) -> Option<usize> {
        let slot = (0..self.slots.len())
            .map(|offset| (self.next + offset) % self.slots.len())
            .find(|&index| self.slots[index].state == SlotState::Free)?;
        self.next = (slot + 1) % self.slots.len();
        self.slots[slot].state = SlotState::Recording;
        stamp(encoder, &self.slots[slot].queries, 0);
        Some(slot)
    }

    /// Close the frame's timestamps and copy them into the readback buffer.
    /// The caller must submit the encoder before calling [`Self::request_map`].
    pub(super) fn end(&mut self, encoder: &mut wgpu::CommandEncoder, slot: usize) {
        let Some(slot) = self.slots.get_mut(slot) else {
            return;
        };
        if slot.state != SlotState::Recording {
            return;
        }
        stamp(encoder, &slot.queries, 1);
        encoder.resolve_query_set(&slot.queries, 0..2, &slot.resolve, 0);
        encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, 16);
        slot.state = SlotState::Pending;
    }

    /// Ask for the readback bytes after the encoder has been submitted. A
    /// buffer with a pending map cannot be written by a submitted encoder.
    pub(super) fn request_map(&mut self, slot: usize) {
        let Some(slot) = self.slots.get_mut(slot) else {
            return;
        };
        if slot.state != SlotState::Pending {
            return;
        }
        slot.mapped.store(MAP_PENDING, Ordering::Release);
        let mapped = Arc::clone(&slot.mapped);
        slot.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                mapped.store(
                    if result.is_ok() {
                        MAP_READY
                    } else {
                        MAP_FAILED
                    },
                    Ordering::Release,
                );
            });
    }

    /// Release a slot whose encoder will not be submitted.
    pub(super) fn cancel(&mut self, slot: usize) {
        if let Some(slot) = self.slots.get_mut(slot)
            && slot.state == SlotState::Recording
        {
            slot.state = SlotState::Free;
        }
    }

    /// Release any slot left recording by a failed encode.
    pub(super) fn cancel_recording(&mut self) {
        for slot in &mut self.slots {
            if slot.state == SlotState::Recording {
                slot.state = SlotState::Free;
            }
        }
    }

    /// Poll without waiting and collect any completed timestamps.
    pub(super) fn poll(&mut self, device: &wgpu::Device, timestamp_period: f32) {
        let _ = device.poll(wgpu::PollType::Poll);
        for slot in &mut self.slots {
            if slot.state != SlotState::Pending {
                continue;
            }
            match slot.mapped.load(Ordering::Acquire) {
                MAP_FAILED => {
                    slot.readback.unmap();
                    slot.mapped.store(MAP_PENDING, Ordering::Release);
                    slot.state = SlotState::Free;
                }
                MAP_READY => {
                    let Ok(bytes) = slot.readback.slice(..).get_mapped_range() else {
                        continue;
                    };
                    let first = u64::from_ne_bytes(bytes[0..8].try_into().unwrap());
                    let last = u64::from_ne_bytes(bytes[8..16].try_into().unwrap());
                    let elapsed =
                        last.wrapping_sub(first) as f64 * f64::from(timestamp_period) / 1e6;
                    drop(bytes);
                    slot.readback.unmap();
                    slot.mapped.store(MAP_PENDING, Ordering::Release);
                    slot.state = SlotState::Free;
                    self.last_ms = Some(elapsed);
                }
                _ => continue,
            }
        }
    }
}

fn stamp(encoder: &mut wgpu::CommandEncoder, queries: &wgpu::QuerySet, index: u32) {
    let _pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("Plico frame budget marker"),
        timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
            query_set: queries,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: None,
        }),
    });
}

impl Renderer {
    /// CPU and GPU work spent on the most recently encoded frame.
    pub fn frame_budget(&self) -> FrameBudget {
        self.frame_budget
    }

    /// Collect completed timestamps and reset the frame's CPU stages.
    pub(super) fn poll_gpu_budget(&mut self) {
        let period = self.queue.get_timestamp_period();
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.cancel_recording();
            let device = &self.device;
            timer.poll(device, period);
            self.frame_budget.gpu_ms = timer.last_ms();
        }
    }

    /// Start timing the root encoder; child-layer command buffers stay in its queue bracket.
    pub(super) fn begin_gpu_budget(&mut self, encoder: &mut wgpu::CommandEncoder) {
        // Child-layer encoders share the frame's bracket; do not replace the
        // root slot while one is active.
        if self.layer_depth != 0 || self.pending_gpu_slot.is_some() {
            return;
        }
        self.pending_gpu_slot = self
            .gpu_timer
            .as_mut()
            .and_then(|timer| timer.begin(encoder));
    }

    /// Close the root encoder's timestamps before it is submitted.
    pub(super) fn end_gpu_budget(&mut self, encoder: &mut wgpu::CommandEncoder) -> Option<usize> {
        let slot = self.pending_gpu_slot.take()?;
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.end(encoder, slot);
        }
        Some(slot)
    }

    /// Request the timing readback after the encoder has been submitted.
    pub(super) fn arm_gpu_budget(&mut self, slot: Option<usize>) {
        if let (Some(slot), Some(timer)) = (slot, self.gpu_timer.as_mut()) {
            timer.request_map(slot);
        }
    }

    /// Drop a timing slot whose encoder will not be submitted.
    #[cfg_attr(not(feature = "window"), allow(dead_code))]
    pub(super) fn cancel_gpu_budget(&mut self) {
        if let Some(slot) = self.pending_gpu_slot.take()
            && let Some(timer) = self.gpu_timer.as_mut()
        {
            timer.cancel(slot);
        }
    }
}
