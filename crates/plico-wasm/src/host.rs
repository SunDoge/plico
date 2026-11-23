use std::path::Path;

pub use wasmtime::{Config, Engine, Error, OptLevel, Result};

/// An instantiated development module. The host controls the protocol and buffer size.
pub struct Module {
    store: wasmtime::Store<()>,
    memory: wasmtime::Memory,
    update: wasmtime::TypedFunc<u32, u32>,
    input: usize,
    output: usize,
    capacity: usize,
}

impl Module {
    pub fn load(
        engine: &Engine,
        path: &Path,
        version_expected: u32,
        capacity: usize,
    ) -> Result<Self> {
        if capacity == 0 || capacity > u32::MAX as usize {
            return Err(Error::msg("invalid protocol buffer capacity"));
        }
        let module = wasmtime::Module::from_file(engine, path)?;
        let mut store = wasmtime::Store::new(engine, ());
        // The portable Plico build contains browser bindings. The reader
        // receives all inputs as values and grants no ambient browser APIs.
        let mut linker = wasmtime::Linker::new(engine);
        linker.define_unknown_imports_as_traps(&module)?;
        let instance = linker.instantiate(&mut store, &module)?;
        let version = instance.get_typed_func::<(), u32>(&mut store, "abi_version")?;
        if version.call(&mut store, ())? != version_expected {
            return Err(wasmtime::Error::msg("incompatible UI module protocol"));
        }
        let exported_capacity = instance
            .get_typed_func::<(), u32>(&mut store, "buffer_capacity")?
            .call(&mut store, ())?;
        if exported_capacity as usize != capacity {
            return Err(Error::msg("incompatible UI buffer capacity"));
        }
        let input = instance
            .get_typed_func::<(), u32>(&mut store, "input_ptr")?
            .call(&mut store, ())? as usize;
        let output = instance
            .get_typed_func::<(), u32>(&mut store, "output_ptr")?
            .call(&mut store, ())? as usize;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| wasmtime::Error::msg("no guest memory export"))?;
        if [input, output].into_iter().any(|offset| {
            offset
                .checked_add(capacity)
                .is_none_or(|end| end > memory.data_size(&store))
        }) || input.abs_diff(output) < capacity
        {
            return Err(wasmtime::Error::msg("invalid guest buffer ranges"));
        }
        let update = instance.get_typed_func(&mut store, "update_ui")?;
        Ok(Self {
            store,
            memory,
            update,
            input,
            output,
            capacity,
        })
    }

    /// Encode directly into guest memory and borrow the reply until the next call.
    pub fn call<I>(&mut self, request: &I) -> Result<(&[u8], usize)>
    where
        I: for<'a, 'b> rkyv::Serialize<
                rkyv::api::high::HighSerializer<
                    rkyv::ser::writer::Buffer<'b>,
                    rkyv::ser::allocator::ArenaHandle<'a>,
                    rkyv::rancor::Error,
                >,
            >,
    {
        self.call_with(|input| {
            crate::write_archive(request, input).map_err(|e| Error::msg(e.to_string()))
        })
    }

    /// Produce a request in guest memory, including fixed-layout payloads authored in place.
    pub fn call_with(
        &mut self,
        write: impl FnOnce(&mut [u8]) -> Result<usize>,
    ) -> Result<(&[u8], usize)> {
        let input =
            &mut self.memory.data_mut(&mut self.store)[self.input..self.input + self.capacity];
        let len = write(input)?;
        if len > self.capacity {
            return Err(Error::msg("invalid request length"));
        }
        let written = self.update.call(&mut self.store, len as u32)? as usize;
        if !(1..=self.capacity).contains(&written) {
            return Err(Error::msg("invalid reply length"));
        }
        Ok((
            &self.memory.data(&self.store)[self.output..self.output + written],
            len,
        ))
    }
}
