//! One host-owned retained image canvas, addressed by the managed declaration.
use super::*;
use plico_renderer::image_canvas::{CanvasEvent, CanvasRegion, CanvasTool};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Region {
    id: u64,
    rect: [f64; 4],
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Description {
    document: u64,
    selected: Option<u64>,
    tool: u32,
    regions: Vec<Region>,
}
pub(super) fn decode(batch: &OwnedBatch) -> Result<Option<(u64, u64, Description)>, String> {
    let mut canvases = batch.commands.iter().filter(|c| c.kind == CANVAS);
    let Some(command) = canvases.next() else {
        return Ok(None);
    };
    if canvases.next().is_some() {
        return Err("only one native image canvas is supported per window".into());
    }
    let input: Description =
        serde_json::from_str(batch.text(command.value_start, command.value_len))
            .map_err(|e| format!("invalid canvas declaration: {e}"))?;
    let mut ids = std::collections::HashSet::new();
    if input.tool > 2
        || input.regions.len() > 10_000
        || input.regions.iter().any(|r| {
            r.id == 0
                || !ids.insert(r.id)
                || !r.rect.iter().all(|v| v.is_finite())
                || r.rect[2] <= r.rect[0]
                || r.rect[3] <= r.rect[1]
        })
    {
        return Err("invalid canvas tool, region IDs or rectangles".into());
    }
    Ok(Some((command.id, command.resource, input)))
}
impl ManagedView {
    pub(super) fn sync_canvas(
        &mut self,
        input: Option<(u64, u64, Description)>,
    ) -> Result<(), String> {
        let Some((node, resource, description)) = input else {
            self.canvas_node = None;
            return Ok(());
        };
        self.canvas_node = Some(node);
        let state = &self.mailbox.canvas;
        let regions = || {
            description
                .regions
                .iter()
                .map(|r| CanvasRegion {
                    id: r.id,
                    rect: r.rect,
                })
                .collect()
        };
        let new_document = self
            .canvas_input
            .as_ref()
            .is_none_or(|last| last.document != description.document);
        if new_document || resource != self.canvas_resource {
            let image = if resource == 0 {
                None
            } else {
                Some(
                    self.images
                        .get(&resource)
                        .ok_or("unknown canvas image")?
                        .raster
                        .clone(),
                )
            };
            if new_document
                || image
                    .as_ref()
                    .is_none_or(|image| !state.replace_image(image.clone()))
            {
                state.set_document(image, regions());
            }
            self.canvas_resource = resource;
        }
        // Do not repeatedly call set_regions: it cancels an in-progress native drag.
        if new_document
            || self.canvas_input.as_ref().is_none_or(|last| {
                last.regions != description.regions || last.selected != description.selected
            })
        {
            state.set_regions(regions(), description.selected);
        }
        let tool = match description.tool {
            0 => CanvasTool::Draw,
            1 => CanvasTool::Select,
            _ => CanvasTool::Pan,
        };
        if state.tool() != tool {
            state.set_tool(tool);
        }
        self.canvas_input = Some(description);
        Ok(())
    }
    pub(super) fn drain_canvas_events(&mut self) {
        let mut events = Vec::new();
        while let Some(event) = self.mailbox.canvas.take_event() {
            events.push(match event {
                CanvasEvent::Added(rect) => serde_json::json!({"kind":"added","rect":rect}),
                CanvasEvent::Edited(region) => {
                    serde_json::json!({"kind":"edited","id":region.id,"rect":region.rect})
                }
                CanvasEvent::Selected(id) => serde_json::json!({"kind":"selected","id":id}),
                CanvasEvent::Delete(id) => serde_json::json!({"kind":"delete","id":id}),
            });
        }
        if !events.is_empty()
            && let Some(id) = self.canvas_node
        {
            self.pending_replies.push(Reply { id, kind: 5, value: self.mailbox.canvas.scale(), text: serde_json::json!({"document":self.canvas_input.as_ref().map(|d|d.document), "events":events}).to_string() });
        }
    }
}
pub(super) fn control(id: u64, operation: u32, value: f64) -> i32 {
    status(|| {
        let mailbox = mailboxes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
            .ok_or("host is closed")?;
        match operation {
            0 => mailbox.canvas.fit(),
            1 => mailbox.canvas.actual_size(),
            2 if value.is_finite() && value > 0. => mailbox.canvas.zoom(value),
            _ => return Err("invalid canvas control".into()),
        }
        Ok(())
    })
}

pub(super) fn viewport(id: u64, out: *mut abi::CanvasViewport) -> i32 {
    status(|| {
        if out.is_null()
            || !(out as usize).is_multiple_of(std::mem::align_of::<abi::CanvasViewport>())
        {
            return Err("invalid viewport output".into());
        }
        with_host(id, |host| {
            let [x, y, width, height] =
                host.source.viewport_rect().ok_or("canvas is not visible")?;
            unsafe {
                out.write(abi::CanvasViewport {
                    x: x.into(),
                    y: y.into(),
                    width: width.into(),
                    height: height.into(),
                    scale: host.source.source.view.mailbox.canvas.scale(),
                });
            }
            Ok(())
        })
    })
}
pub(super) fn pointer(id: u64, phase: u32, x: f64, y: f64, pressed: bool) -> i32 {
    status(|| {
        use plico_renderer::{
            Point, PointerButton, PointerEvent, PointerPhase, PointerProperties, UiEvent,
        };
        if !x.is_finite() || !y.is_finite() {
            return Err("invalid pointer position".into());
        }
        let phase = match phase {
            0 => PointerPhase::Move,
            1 => PointerPhase::Down,
            2 => PointerPhase::Up,
            3 => PointerPhase::Leave,
            _ => return Err("invalid pointer phase".into()),
        };
        with_host(id, |host| {
            host.source.prepare_for_event(&mut host.text);

            host.source.handle_event(UiEvent::Pointer(PointerEvent {
                phase,
                position: Point { x, y },
                button: Some(PointerButton::Primary),
                buttons: u32::from(pressed),
                modifiers: Default::default(),
                properties: PointerProperties::default(),
            }));
            host.source.poll_async();
            Ok(())
        })
    })
}
