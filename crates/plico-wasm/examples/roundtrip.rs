//! Both sides borrow variable-length fields directly in the guest's linear memory.
#[derive(rkyv::Archive, rkyv::Serialize)]
pub struct Request {
    values: Vec<u32>,
}

#[derive(rkyv::Archive, rkyv::Serialize)]
pub struct Reply {
    label: String,
    sum: u32,
}

#[cfg(target_arch = "wasm32")]
plico_wasm::export_ui!(2, 256, Request, |input: &ArchivedRequest, out| {
    let sum = input
        .values
        .iter()
        .map(|n| n.to_native())
        .fold(0u32, u32::wrapping_add);
    let reply = Reply {
        label: format!("{} values", input.values.len()),
        sum,
    };
    plico_wasm::write_archive(&reply, out).map_or(0, |len| len as u32)
});

#[cfg(all(not(target_arch = "wasm32"), feature = "host"))]
fn main() -> plico_wasm::Result<()> {
    use plico_wasm::{Engine, Module};
    let path = std::env::args_os().nth(1).expect("guest module path");
    let path = std::path::Path::new(&path);
    let engine = Engine::default();
    assert!(Module::load(&engine, path, 1, 256).is_err());
    assert!(Module::load(&engine, path, 2, 32).is_err());
    let mut guest = Module::load(&engine, path, 2, 256)?;
    assert!(
        guest
            .call(&Request {
                values: vec![0; 256]
            })
            .is_err()
    );
    assert!(guest.call_with(|buffer| Ok(buffer.len() + 1)).is_err());
    assert!(
        guest
            .call_with(|buffer| {
                buffer[0] = 0xff;
                Ok(1)
            })
            .is_err()
    );
    for values in [vec![], vec![41, 1], vec![u32::MAX, 1]] {
        let sum = values.iter().copied().fold(0u32, u32::wrapping_add);
        let label = format!("{} values", values.len());
        let (reply, _) = guest.call(&Request { values })?;
        let view = rkyv::access::<ArchivedReply, rkyv::rancor::Error>(reply)
            .map_err(|error| plico_wasm::Error::msg(error.to_string()))?;
        assert_eq!(view.sum.to_native(), sum);
        assert_eq!(view.label.as_str(), label);
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", not(feature = "host")))]
fn main() {}
