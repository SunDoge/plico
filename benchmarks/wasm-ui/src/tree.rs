use super::ReaderState;

pub const TREE_REVISION: u32 = super::REVISION + cfg!(feature = "reader-service-revised") as u32;
use plico::prelude::*;
#[cfg(any(target_arch = "wasm32", feature = "wasm-host"))]
use plico::{PathKey, ViewNode};

#[cfg(any(target_arch = "wasm32", feature = "wasm-host"))]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Input {
    pub reader: ReaderState,
    pub selected: bool,
    pub clicks: Vec<Vec<PathKey>>,
    #[cfg(feature = "reader-service")]
    pub analysis: super::service::Snapshot,
}

#[cfg(any(target_arch = "wasm32", feature = "wasm-host"))]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Output {
    pub revision: u32,
    pub selected: bool,
    pub nodes: Vec<ViewNode>,
    #[cfg(feature = "reader-service")]
    pub commands: Vec<super::service::Command>,
}

pub fn show(
    cx: &mut UiContext<'_>,
    state: ReaderState,
    selected: &mut bool,
    #[cfg(feature = "reader-service")] services: &mut super::service::ReaderServices,
) {
    let marker = if TREE_REVISION % 2 == 0 {
        0xcc2233
    } else {
        0x22cc55
    };
    row(cx)
        .gap(8)
        .height(32.)
        .align(Align::Center)
        .children(|cx| {
            column(cx)
                .width(32.)
                .height(32.)
                .background(Color::Literal(marker))
                .end();
            #[cfg(feature = "reader-service")]
            text!(cx, "Page {} · zoom {}%", state.page, state.zoom);
            #[cfg(not(feature = "reader-service"))]
            text!(
                cx,
                "Page {} · zoom {}% · OCR {}%",
                state.page,
                state.zoom,
                state.progress
            );
            if button(cx)
                .text(if *selected { "Marked" } else { "Mark" })
                .clicked()
            {
                *selected = !*selected;
            }
        });
    #[cfg(feature = "reader-service")]
    row(cx)
        .gap(8)
        .align(Align::Center)
        .children(|cx| super::service::show(cx, state.image, services));
    column(cx)
        .position(Position::Relative)
        .width(376.)
        .height(480.)
        .children(|cx| {
            for index in 0..state.regions {
                cx.keyed(u64::from(index), |cx| {
                    column(cx)
                        .absolute((index * 47 % 320) as f32, (index * 59 % 450) as f32)
                        .width(24.)
                        .height(14.)
                        .border(1., Color::Literal(0x2c83e4))
                        .end();
                });
            }
        });
}

pub fn declare(
    engine: &mut plico::Engine,
    state: ReaderState,
    selected: &mut bool,
    #[cfg(feature = "reader-service")] services: &mut super::service::ReaderServices,
) {
    engine.set_viewport(420., 570.).unwrap();
    for _ in 0..2 {
        engine
            .frame_declarations(|cx| {
                show(
                    cx,
                    state,
                    selected,
                    #[cfg(feature = "reader-service")]
                    services,
                )
            })
            .unwrap();
        if !engine.changes().contains(plico::Changes::VIEW) {
            break;
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn update(engine: &mut plico::Engine, input: &ArchivedInput) -> Output {
    for path in input.clicks.iter() {
        engine.queue_click_path(rkyv::deserialize::<_, rkyv::rancor::Error>(path).unwrap());
    }
    let mut selected = input.selected;
    #[cfg(feature = "reader-service")]
    let mut services = super::service::ReaderServices::new(
        rkyv::deserialize::<_, rkyv::rancor::Error>(&input.analysis).unwrap(),
    );
    declare(
        engine,
        rkyv::deserialize::<_, rkyv::rancor::Error>(&input.reader).unwrap(),
        &mut selected,
        #[cfg(feature = "reader-service")]
        &mut services,
    );
    Output {
        revision: TREE_REVISION,
        nodes: engine.snapshot(),
        selected,
        #[cfg(feature = "reader-service")]
        commands: services.take_commands(),
    }
}

#[cfg(feature = "wasm-host")]
pub fn replay(
    cx: &mut UiContext<'_>,
    nodes: &[ViewNode],
    path: &mut Vec<PathKey>,
    clicks: &mut Vec<Vec<PathKey>>,
) {
    use std::hash::{Hash, Hasher};
    for node in nodes {
        path.push(node.key.clone());
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        node.key.hash(&mut hasher);
        let key = plico::Key::Number(hasher.finish());
        if node.kind == plico::Kind::Text {
            cx.rich_text_element(key, node.style, &node.text, &node.spans);
        } else {
            let (_, clicked) = cx.element_with_grid(
                key,
                node.kind,
                node.style,
                &node.text,
                node.grid.as_ref(),
                |cx| {
                    replay(cx, &node.children, path, clicks);
                },
            );
            if clicked {
                clicks.push(path.clone());
            }
        }
        path.pop();
    }
}

#[cfg(all(
    feature = "tree-patch",
    any(target_arch = "wasm32", feature = "wasm-host")
))]
pub mod patch {
    use super::*;

    #[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    pub struct Patch {
        revision: u32,
        selected: bool,
        edits: plico_dev::tree_update::TextPatch,
        #[cfg(feature = "reader-service")]
        commands: Vec<super::super::service::Command>,
    }

    #[cfg(target_arch = "wasm32")]
    pub fn between(previous: Option<&Output>, next: &Output) -> Option<Patch> {
        if let Some(edits) = previous
            .and_then(|old| plico_dev::tree_update::TextPatch::between(&old.nodes, &next.nodes))
        {
            Some(Patch {
                revision: next.revision,
                selected: next.selected,
                edits,
                #[cfg(feature = "reader-service")]
                commands: next.commands.clone(),
            })
        } else {
            None
        }
    }

    #[cfg(feature = "wasm-host")]
    pub fn apply(current: &mut Option<Output>, patch: &ArchivedPatch) {
        let frame = current.as_mut().expect("full frame before its patches");
        let edits: plico_dev::tree_update::TextPatch =
            rkyv::deserialize::<_, rkyv::rancor::Error>(&patch.edits).unwrap();
        edits.apply(&mut frame.nodes).unwrap();
        frame.revision = patch.revision.to_native();
        frame.selected = patch.selected;
        #[cfg(feature = "reader-service")]
        {
            frame.commands = rkyv::deserialize::<_, rkyv::rancor::Error>(&patch.commands).unwrap();
        }
    }
}
