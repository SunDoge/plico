# Plico's temporary Glifo patch

Source: crates.io `glifo` 0.4.0, upstream https://github.com/linebender/vello.
Original MIT and Apache-2.0 licenses and copyright headers are retained.

The `precise-hinted-outlines` feature (enabled by default in this patched copy) flattens hinted glyph outlines at
0.005 draw pixels when inserting them into the existing outline cache, after
hinting and synthetic emboldening. It avoids vello_common 0.3.0's coarser curve
approximation. Cache hits reuse the path; placement and hinting are unchanged.
Unhinted outlines (cached in font units), bitmap strikes, and general UI paths
are unchanged. Large additional glyph transforms can magnify the tolerance.

The root Cargo patch selects this copy for the workspace. This is a temporary
workaround, not an independently maintained text engine. Remove it when the
upstream renderer provides adequate glyph curve precision. Publishing crates
with this behavior requires making the patched dependency available to
consumers; a workspace `[patch]` does not propagate through a published crate.
The workspace dependency must not request the custom feature by name: external
generated apps resolve those inherited dependencies against upstream Glifo,
where the local feature does not exist. They build normally but need their own
root patch to get this workaround when rendering outside Plico's native host.

Regression command: `python3 scripts/check-text-artifacts.py`.
Independent precision experiment: `python3 scripts/compare-s-rasterizers.py`.
Phase scan: `python3 scripts/scan-s-phases.py`.

Upstream unit-test font fixtures are not present in the crates.io source
archive. Validate the patch through Plico's production renderer tests/probes;
running Glifo's own unit tests requires the upstream test font assets.
