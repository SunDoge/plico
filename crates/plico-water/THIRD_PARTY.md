# Third-party design reference

The ping-pong height/velocity simulation is based on the technique demonstrated
by Evan Wallace's WebGL Water project at <https://github.com/evanw/webgl-water>
(upstream revision `73eda8be832b649367b25ea5690c1f0181bb56ad`). The original is MIT licensed;
copyright belongs to Evan Wallace. Plico's implementation uses independent Rust
host code and WGSL shaders designed for its persistent external-surface path.

The upstream MIT license is retained in `licenses/webgl-water-MIT.txt`.

The embedded JPEG assets are byte-for-byte copies from that recorded upstream
revision, stored as base64 so they remain reviewable through text-only patch
tooling. The pool tile is credited by the upstream page to Flickr user
`zooboing` at <https://www.flickr.com/photos/zooboing/3682834083/>. The sky
cubemap faces are distributed with the upstream WebGL Water repository.
