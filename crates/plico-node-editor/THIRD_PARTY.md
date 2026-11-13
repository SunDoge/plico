# Third-party design references

The editor's pointer-drag behavior and transformed-coordinate test strategy are
informed by MyGO's `ui/draggable.go`, `ui/draggable_test.go`, `ui/input.go` and
`ui/path.go` at the locally recorded upstream revision. MyGO is MIT licensed;
copyright belongs to the MyGO contributors. Plico's implementation is an
independent Rust composition over its existing immediate and retained APIs.

The complete MIT license is retained at `crates/plico/third_party/mygo-LICENSE`.
