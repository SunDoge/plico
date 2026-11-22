//! Exercise the exact browser guest artifacts on the native Wasmtime host.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> plico_wasm::Result<()> {
    use plico::{Kind, PathKey, ViewNode};
    use web_demo::protocol::{ABI, CAPACITY, Input, Reply, ServiceSnapshot};

    fn increment_path(nodes: &[ViewNode], path: &mut Vec<PathKey>) -> bool {
        for node in nodes {
            path.push(node.key.clone());
            if (matches!(node.kind, Kind::Button | Kind::ButtonBase) && node.text == "Increment")
                || increment_path(&node.children, path)
            {
                return true;
            }
            path.pop();
        }
        false
    }
    let directory = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("split host build directory"),
    );
    let engine = plico_wasm::Engine::default();
    let mut checkpoint = Default::default();
    let mut path = Vec::new();
    for (variant, counts) in [("v1", &[0, 1, 2, 3][..]), ("v2", &[3, 4]), ("reset", &[0])] {
        let mut module = plico_wasm::Module::load(
            &engine,
            &directory.join(format!("ui-{variant}.wasm")),
            ABI,
            CAPACITY,
        )?;
        let mut previous = Vec::new();
        let mut base = None;
        for (i, &count) in counts.iter().enumerate() {
            let request = Input {
                state: checkpoint,
                // Simulate an unacknowledged frame before the third click.
                base: if i == 3 { Some(1) } else { base },
                viewport: Some([720., 580.]),
                services: ServiceSnapshot {
                    instance: 1,
                    progress: 1,
                },
                clicks: if i == 0 { vec![] } else { vec![path.clone()] },
                ..Default::default()
            };
            let (bytes, _) = module.call(&request)?;
            let reply = rkyv::from_bytes::<Reply, rkyv::rancor::Error>(bytes).unwrap();
            assert_eq!(
                matches!(reply.nodes, plico_dev::tree_update::TreeUpdate::Text(_)),
                i > 0 && i != 3
            );
            let frame = reply.resolve(request.base, &mut previous).unwrap();
            base = Some(frame.revision);
            plico_dev::validate_nodes(&frame.nodes).unwrap();
            assert_eq!(
                frame.state.decode::<serde_json::Value>().unwrap()["count"],
                count
            );
            assert_eq!(frame.reset.is_some(), variant == "reset");
            checkpoint = frame.state;
            path.clear();
            assert!(increment_path(&frame.nodes, &mut path));
            previous = frame.nodes;
        }
    }
    // Compare the same service updates in one process. Instantiate before timing
    // and discard warmup calls to exclude compilation and first-frame costs.
    for incremental in [false, true] {
        let mut module =
            plico_wasm::Module::load(&engine, &directory.join("ui-v1.wasm"), ABI, CAPACITY)?;
        let mut base = None;
        let mut previous = Vec::new();
        let mut samples = Vec::new();
        let mut total_bytes = 0;
        let mut state = Default::default();
        for progress in 0..220 {
            let request = Input {
                base,
                state,
                services: ServiceSnapshot {
                    instance: 1,
                    progress,
                },
                ..Default::default()
            };
            let start = std::time::Instant::now();
            let (bytes, _) = module.call(&request)?;
            let size = bytes.len();
            let reply = rkyv::from_bytes::<Reply, rkyv::rancor::Error>(bytes).unwrap();
            let frame = reply.resolve(base, &mut previous).unwrap();
            base = incremental.then_some(frame.revision);
            state = frame.state;
            previous = frame.nodes;
            if progress >= 20 {
                samples.push(start.elapsed());
                total_bytes += size;
            }
        }
        samples.sort_unstable();
        println!(
            "incremental={incremental}: {} updates, {:.1} bytes/update, median {:.1} us",
            samples.len(),
            total_bytes as f64 / samples.len() as f64,
            samples[samples.len() / 2].as_secs_f64() * 1e6
        );
    }
    println!("Counter clicks, compatible reload and schema reset passed on Wasmtime");
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
