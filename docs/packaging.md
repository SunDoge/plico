# Native application bundles

`plico bundle apps/gallery` builds the application's ordinary release binary,
with default features. It does not ship the development host or reload module.
Choose an application directory, as with `plico run`. Multiple binary targets
require `default-run` or `--bin`. `--features` and `--no-default-features` have
Cargo semantics. Builds use `--locked`; create/update Cargo.lock beforehand.

Default output: `target/bundle/<package>-<os>-<arch>/`. Override with
`--output <directory>` (relative to the application directory). Existing output
is rejected rather than overwritten. A temporary sibling directory is published
only after resource collection succeeds.

Declare external resources in the application's Cargo.toml:

```toml
[package.metadata.plico.bundle]
resources = ["assets"]
```

Paths are relative to the package, copied under `resources/`, and may name files
or directories. Symlinks, special files and parent traversal are rejected.
`include_bytes!`/`include_str!` resources already live in the binary and need no
entry. Resource files must not contain secrets intended only for development.

On Unix, run `./launch`. It exports `PLICO_RESOURCE_DIR` (an absolute path), sets
the working directory to `resources/`, and forwards arguments to `bin/<binary>`.
Applications should resolve resources from that environment variable with a
suitable development fallback. Windows currently supplies the binary and resource
directory only; the Unix launcher is not generated.

Linux also gets a `.desktop` file whose Exec points at this bundle's absolute
launcher path. Regenerate it after moving the bundle; it is not automatically
installed in the desktop menu. `bundle.json` records package/version/platform.

This is a native directory bundle, not an AppImage, deb/rpm, signed macOS .app,
Windows installer, or static Linux distribution. Build on the intended platform.
System library dependencies remain those of the application (GPU/windowing,
fontconfig, etc.). Build on an appropriately old distribution for Linux releases;
this command does not solve glibc compatibility or copy shared libraries.
Code signing, icons, installer integration and archive generation are not yet
implemented.

Validation: `cargo build -p plico-cli && scripts/check-bundle.sh` builds a separate
small application, packages a resource, starts from `/` with a spaced output path,
checks argument forwarding and verifies overwrite rejection.
