# plico-async (experimental)

Optional coroutine components using Plico's existing synchronous widgets and a
single shared Engine. This is an experiment, not the default application API.

```sh
cargo run -p plico-async --example mixed
cargo test -p plico-async
```

```rust
async fn counter(ui: AsyncUi) -> plico_async::Result<()> {
    let mut count = 0;
    loop {
        ui.frame(|cx| {
            text!(cx, "Count: {count}");
            if button(cx).text("Increment").clicked() {
                count += 1;
                cx.request_rebuild();
            }
        }).await?;
    }
}
```

A synchronous parent mounts it with
`mounts.mount(column(cx).key("counter"), counter)`. Stable parent/key identity
preserves the Future and local state (existing `#[component]` call-site keys also
work for synchronous mounting); removing the anchor cancels its subtree.
The same mount operation is available inside an async component's frame through
`ui.mount(...)`. No new macro, TLS pointer, signal graph or second widget API.

`Input<T>` is an explicit shared input boundary: subscribe a scope once, then
read it inside the actual frame closure. Component-local values are ordinary Rust
variables. `while_pending` draws a waiting view and resumes on a host Future's
completion. `while_pending_or` can instead return a UI action (such as Cancel)
and drop the waiting Future. It does not create an IO executor or make blocking work asynchronous.

Selected `ui.state` values can be restored from MessagePack checkpoints; unmarked
locals and execution positions reset. Checkpoints remain explicit. There is no
native dynamic-library integration in this crate yet.

Hosts must bridge notifications to their event loop and route input to the common
Engine. Use `with_wake(..., true)` for external layout; the default only uses core's
simulated layout. A renderer adapter is not supplied. Theme/viewport changes mark
the synchronous parent dirty and wake scopes; hosts must render the parent again.

**Limits:** subtree commit/layout and lifecycle reconciliation still scan retained
storage. This is not O(delta), and large simultaneous scope updates can repeat
those scans. Scope anchors are real layout containers, not transparent fragments.
Portals escaping an anchor are rejected. Focus restoration, offscreen scope
suspension, navigation persistence policies and actual backend integration are
not complete. A finished Future clears its UI; preserve a terminal view by waiting
in a final frame loop. Same-key factories are not rerun to update captured props.

See [design and validation](../../docs/async-components.md).
