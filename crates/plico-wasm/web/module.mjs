// Instantiate a trusted UI module with the same buffer contract as the native host.
// A returned view borrows guest memory until the next call.
export async function openUiModule(binary, version, capacity) {
  const module = await WebAssembly.compile(binary);
  const imports = Object.create(null);
  for (const entry of WebAssembly.Module.imports(module)) {
    if (entry.kind !== "function") throw new Error("Unsupported UI import: " + entry.name);
    const namespace = imports[entry.module] ??= Object.create(null);
    namespace[entry.name] = () => { throw new Error("Unavailable UI import: " + entry.name); };
  }
  const {exports: ui} = await WebAssembly.instantiate(module, imports);
  if (ui.abi_version() !== version || !Number.isSafeInteger(capacity) || capacity <= 0 ||
      (ui.buffer_capacity() >>> 0) !== capacity) {
    throw new Error("Incompatible UI protocol");
  }
  const input = ui.input_ptr() >>> 0;
  const output = ui.output_ptr() >>> 0;
  if (typeof ui.update_ui !== "function" || !(ui.memory instanceof WebAssembly.Memory) ||
      Math.max(input, output) + capacity > ui.memory.buffer.byteLength ||
      Math.abs(input - output) < capacity) {
    throw new Error("Invalid UI buffers");
  }
  return request => {
    if (!(request instanceof Uint8Array) || request.byteLength > capacity) {
      throw new Error("Invalid UI request");
    }
    new Uint8Array(ui.memory.buffer, input, capacity).set(request);
    const written = ui.update_ui(request.byteLength) >>> 0;
    if (!written || written > capacity) throw new Error("Invalid UI reply");
    // update_ui may grow memory and detach the previous ArrayBuffer.
    return new Uint8Array(ui.memory.buffer, output, written);
  };
}
