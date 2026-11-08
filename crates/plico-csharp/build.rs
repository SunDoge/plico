fn main() {
    println!("cargo:rerun-if-changed=src/abi.rs");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("Native.g.cs");
    csbindgen::Builder::default()
        .input_extern_file("src/abi.rs")
        .csharp_dll_name("plico_csharp")
        .csharp_namespace("Plico.Interop")
        .csharp_class_name("Native")
        .generate_csharp_file(&output)
        .expect("generate Plico C# bindings");
    let bytes = std::fs::read(output).unwrap();
    let target = "dotnet/Plico.CSharp/Native.g.cs";
    if std::fs::read(target).ok().as_deref() != Some(bytes.as_slice()) {
        std::fs::write(target, bytes).unwrap();
    }
}
