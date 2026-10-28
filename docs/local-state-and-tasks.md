# Synchronous local state and host tasks

UI remains ordinary synchronous `App::show`.

## Component-local state

```rust
#[component]
fn counter(cx: &mut UiContext) {
    let count = cx.state(|| 0_u32);
    text!(cx, "Count: {}", count.get());
    if button(cx).text("Increment").clicked() {
        count.update(|n| *n += 1);
    }
}
```

`state` returns a UI-thread-only handle, not a borrow of the context. The value
lives in Engine, outside serialized application fields. Identity combines the
parent entity, component namespace and Rust caller location; it does not depend
on hook order. Repeated instances need `cx.keyed(business_id, ...)`, just like
other immediate declarations. Distinct calls on the same source line still
have distinct columns. A wrapper around `state` should itself use `#[track_caller]`.

`read()` borrows without cloning; `get()` explicitly clones. `set()` compares
before replacing. `update()` clones a temporary value, applies the closure, then
compares/commits; use `set()` for large values to avoid this copy. Changes request
another declaration through the existing settle loop. Equal writes do not.
Drop a `read()` guard before writing the same slot. There is no dependency graph
and no per-widget subscription. Construction allocates one Rc/RefCell per slot;
repeated lookup clones the Rc, not the stored value. Caller-file identity uses a
static string, without allocating a string per frame.

After a successful full declaration, slots of absent components are released.
If you retain a handle externally, it extends that value's life but does not
reattach it to a remounted component. Prefer short-lived declaration handles.

Ordinary `state` currently resets on reload, source relocation or Engine replacement.
It does not implement checkpoint serialization. Keep durable/shared application
data in App or a host service. Values with the same call-site identity must keep
the same Rust type. Initialization is lazy, not repeated each declaration.

## Host-owned tasks

`plico_dev::tasks::Task<T, E>` belongs to a `HostService`, not to the reloadable
module or a JSON/MessagePack snapshot:

```rust,ignore
// In the host service, when handling a save command:
let future = self.save.start(async move { api.save(document).await });
self.executor.spawn(future);

// HostService::poll, after its wake callback posts a host event:
let changed = self.save.poll();
// Project self.save.status() into your existing value-only service snapshot.
```

Supply the service's wake callback to `Task::new`, or use `Task::default()` and
forward `HostService::set_wake_callback` to `Task::set_wake_callback`. Updating
the callback also updates already pending tasks. `start` returns an owned Future
that does not borrow Task; it can run on another thread when its inputs/results
are Send. The facility does not install an executor, create a thread or poll IO
on the UI thread. The app still consumes a typed service snapshot and emits
commands, using the existing service bridge.

States are Idle, Pending, Ready(Result<T, E>) and Cancelled. Completion wakes the
host once, and `poll` drains it. Replacing, cancelling, resetting or dropping a
task invalidates older completions, including their late wake notifications.
Dropping the returned Future reports cancellation when still current. Cancellation
invalidates results; retain and abort your executor's handle if you also want to
stop polling the underlying operation. A server-side effect cannot be rolled back
by cancelling its receiver. Poll completion before exposing a fresh snapshot.

Keeping Task in the host permits it to survive UI-module reload without moving
Futures across the ABI. `TaskStatus<T, E>` and `TaskCommand<I>` are serializable. A single-task app can
use these directly as its `ServiceApp::Snapshot` and `ServiceApp::Command`:

```rust,ignore
let mut save = services.task();
if button(cx).text("Save").disabled(save.pending()).clicked() {
    save.start(document.clone());
}
match save.status() {
    TaskStatus::Idle => {}
    TaskStatus::Pending => {
        text(cx, "Saving...");
        if button(cx).text("Cancel").clicked() { save.cancel(); }
    }
    TaskStatus::Ready(Ok(result)) => { text(cx, result); }
    TaskStatus::Ready(Err(error)) => { text(cx, error); }
    TaskStatus::Cancelled => { text(cx, "Cancelled"); }
}
```

`TaskService::new(operation, spawn)` implements HostService for this protocol.
It runs the async operation on the supplied executor, maintains snapshot revisions,
and forwards wake notifications. The operation factory should only construct a
Future; put IO and expensive work inside it. Inputs/results/errors in the service
protocol must support serde, and result/error snapshots must support Clone.
TaskUi borrows the existing snapshot/outbox, with no per-declaration task creation.
Commands take effect when the host receives the outbox; status during the current
declaration is still the input snapshot. Larger apps can compose TaskService
instances and project their statuses into their own Snapshot/Command enums.

Run `cargo run -p plico-dev --example task_service` for a headless UI/host round
trip with success, failure, cancellation and pending-task survival across UI
session recreation. It uses a deterministic mock executor, not an HTTP backend.
This is a service-based integration; it does not inject an executor into `cx`.

Validation: core local-state tests cover keyed instances, conditional calls,
unmount/reset, equal writes and non-Clone value destruction. Task tests cover
completion, replacement, dropped futures, cancellation, late results, host drop
and background-thread completion. `local_state` is a runnable core example.

## Persistent local storage

```rust
let draft = cx.persisted_state("draft", String::new);
let expanded = cx.persisted_state("advanced", || false);
```

This returns the same `LocalState<T>` read/set/update API. Unlike `state`, the
slot survives route/component unmount. The explicit name replaces source-line
identity; the logical parent path and component namespace still isolate instances.
Dynamic lists need business keys. Keep that namespace/path stable if storage must
survive source edits; this is best-effort recovery, not a permanent schema ID.
Equal names within the same namespace intentionally refer to the same slot.

Values require Serialize + DeserializeOwned, but are stored as ordinary typed
values in memory. Serialization happens only on export/checkpoint, not on reads,
writes or declarations. MessagePack supports types such as non-string map keys
without projecting them into JSON. A slot with incompatible saved data falls back
to its initializer independently, with diagnostics available through
`Engine::take_persisted_resets()`. Unopened slots are preserved during re-export.

```rust,ignore
// Host startup, before declaring UI:
engine.load_persisted(&saved_bytes)?;
// Host-chosen save boundary (perform file/browser IO outside show):
let bytes = engine.dump_persisted()?;
```

The framework does not pick a file path, synchronously write a file on each click,
or automatically use browser localStorage. The host owns storage location and
save timing. Snapshots are versioned. Malformed imports, unknown versions and
duplicate keys fail before replacing live storage. Loading replaces the entire
persistent store; existing handles remain attached to their previous values.

Reloadable `plico_dev` sessions automatically include persistent storage in their
opaque StateSnapshot checkpoint. Public `StateSnapshot::decode` continues to
return only the application state. Plain older App checkpoints remain readable;
an old module that cannot read the new session envelope may reset on downgrade.
Even an incompatible App schema can reset while retaining decodable storage slots.
Lean frames do not serialize storage; a checkpoint request captures current slots.
Per-slot reset diagnostics appear in the Frame's existing reset message.

Cleanup is explicit:

- `cx.remove_persisted_state("draft")`: remove one slot in this namespace.
- `cx.clear_persisted_scope()`: clear this exact parent/component namespace.
- `engine.clear_persisted()`: clear all persistent storage.

Slots do not expire automatically, so applications removing business objects
should clear their stored UI state when appropriate. Clearing does not invalidate
outstanding handles, but those handles no longer attach to future declarations.
Calling remove/clear unconditionally inside show will naturally reset repeatedly;
use an explicit user action or host cleanup boundary.

Run `cargo run -p plico --example persisted_state` for unmount and Engine
recreation. Tests also cover key isolation, unopened entries, incompatible types,
failed import atomicity and checkpoint recovery through the real dev protocol.
