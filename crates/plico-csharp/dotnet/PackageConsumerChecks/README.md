# SDK package integration check

This deliberately references only the packed SDK, with no analyzer reference or
Interceptor properties. It verifies that the distributed package wires component
identity automatically, isolates repeated counters, handles clicks and rebuilds,
and releases local state on unmount. It requires the matching native library.

From the repository root (use a fresh absolute package cache to avoid testing an
older package with the same version):

```sh
dotnet pack crates/plico-csharp/dotnet/Plico.CSharp/Plico.CSharp.csproj -c Release -o /tmp/plico-sdk-pack
dotnet restore crates/plico-csharp/dotnet/PackageConsumerChecks --source /tmp/plico-sdk-pack --packages /tmp/plico-sdk-package-cache
export PLICO_NATIVE_LIBRARY="$PWD/target/release/libplico_csharp.so"
dotnet run --project crates/plico-csharp/dotnet/PackageConsumerChecks -c Release --no-restore
```

The SDK package does not yet ship native runtime binaries. No package is published
by these commands. Run this check explicitly after packaging changes.
