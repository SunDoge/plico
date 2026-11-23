#[cfg(target_arch = "wasm32")]
#[path = "../ui.rs"]
mod ui;

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::ui::*;
    #[cfg(feature = "tree")]
    use std::cell::RefCell;

    #[cfg(not(feature = "tree"))]
    plico_wasm::export_ui!(
        ABI,
        BUFFER_CAPACITY,
        ReaderState,
        |input: &ArchivedReaderState, output| {
            if input.regions.to_native() > 512 {
                return 0;
            }
            let state = rkyv::deserialize::<_, rkyv::rancor::Error>(input).unwrap();
            plico_wasm::write_archive(&update(state), output).map_or(0, |n| n as u32)
        }
    );

    #[cfg(feature = "tree")]
    plico_wasm::export_ui!(ABI, BUFFER_CAPACITY, tree::Input, render_tree);

    #[cfg(feature = "tree")]
    fn render_tree(input: &tree::ArchivedInput, output: &mut [u8]) -> u32 {
        thread_local! {
            static ENGINE: RefCell<plico::Engine> = RefCell::new(plico::Engine::default());
        }
        if input.reader.regions.to_native() > 512 {
            return 0;
        }
        let response = ENGINE.with_borrow_mut(|engine| tree::update(engine, input));
        #[cfg(not(feature = "tree-patch"))]
        return plico_wasm::write_archive(&response, output).map_or(0, |n| n as u32);
        #[cfg(feature = "tree-patch")]
        {
            thread_local! { static PREVIOUS: RefCell<Option<tree::Output>> = const { RefCell::new(None) }; }
            PREVIOUS.with_borrow_mut(|previous| {
                let (tag, bytes) = output.split_first_mut().unwrap();
                let written =
                    if let Some(patch) = tree::patch::between(previous.as_ref(), &response) {
                        *tag = 1;
                        plico_wasm::write_archive(&patch, bytes)
                    } else {
                        *tag = 0;
                        plico_wasm::write_archive(&response, bytes)
                    };
                written.map_or(0, |n| {
                    *previous = Some(response);
                    n as u32 + 1
                })
            })
        }
    }
}

fn main() {}
