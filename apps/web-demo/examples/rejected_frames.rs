//! Generate structurally valid archives for the browser activation rollback test.
use std::{fs, path::PathBuf};
use web_demo::protocol::{Input, render};

fn main() {
    let dir = PathBuf::from(std::env::args_os().nth(1).expect("fixture directory"));
    let frame = render(Input::default()).unwrap();
    let mut frame = frame.map_nodes(plico_dev::tree_update::TreeUpdate::Full);
    frame.version += 1;
    fs::write(
        dir.join("protocol"),
        rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap(),
    )
    .unwrap();
    frame.version -= 1;
    if let plico_dev::tree_update::TreeUpdate::Full(nodes) = &mut frame.nodes {
        nodes.push(nodes[0].clone());
    }
    fs::write(
        dir.join("tree"),
        rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap(),
    )
    .unwrap();
}
