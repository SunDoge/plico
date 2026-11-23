#[cfg(any(target_arch = "wasm32", feature = "wasm-host"))]
pub const ABI: u32 =
    16 + if cfg!(feature = "tree-patch") {
        3
    } else if cfg!(feature = "tree") {
        2
    } else {
        1
    } + if cfg!(feature = "reader-service") {
        4
    } else {
        0
    };
#[cfg(any(target_arch = "wasm32", feature = "wasm-host"))]
pub const BUFFER_CAPACITY: usize = if cfg!(feature = "tree") {
    1024 * 1024
} else {
    64 * 1024
};
pub const REVISION: u32 = 0;

#[cfg_attr(
    any(target_arch = "wasm32", feature = "wasm-host"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Clone, Copy)]
pub struct ReaderState {
    pub page: u32,
    pub image: u32,
    pub zoom: u32,
    pub job_id: u32,
    pub progress: u32,
    pub regions: u32,
}

#[cfg(not(feature = "tree"))]
#[cfg_attr(
    any(target_arch = "wasm32", feature = "wasm-host"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Clone, Copy)]
pub struct Overlay {
    pub x: u16,
    pub y: u16,
}

#[cfg(not(feature = "tree"))]
#[cfg_attr(
    any(target_arch = "wasm32", feature = "wasm-host"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ReaderUi {
    pub title: String,
    pub revision: u32,
    pub state: ReaderState,
    pub overlays: Vec<Overlay>,
}

#[cfg(not(feature = "tree"))]
pub fn update(state: ReaderState) -> ReaderUi {
    ReaderUi {
        title: format!(
            "Page {} · zoom {}% · OCR {}%",
            state.page, state.zoom, state.progress
        ),
        revision: REVISION,
        state,
        overlays: (0..state.regions)
            .map(|index| Overlay {
                x: (index * 47 % 320) as u16,
                y: (index * 59 % 450) as u16,
            })
            .collect(),
    }
}

#[cfg(feature = "tree")]
#[path = "tree.rs"]
pub mod tree;

#[cfg(feature = "reader-service")]
#[path = "service.rs"]
pub mod service;
