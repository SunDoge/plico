# Sources and design references

Text selection interactions in `src/selection.rs` were adapted from the unit-based
anchor/drag design in MyGO's
[`ui/textselection.go`](https://github.com/egoist/mygo/blob/7e8688284e4b1fc752169ed9db5f51a5f213a2e6/ui/textselection.go)
(MIT). Character/word/paragraph units retain their initial range during reverse
drags. Plico uses explicit retained paragraph IDs, UTF-8 offsets, Rust Unicode
segmentation and its injected clock rather than MyGO's Go editor/runtime objects.
The original MIT notice is retained in `licenses/mygo-MIT.txt`.
No MyGO checkout is needed to build or run Plico.

Editable input gestures in `src/input.rs` also reference MyGO
`ui/editor_navigation.go` (`press`, `drag`, `lineEdges`), commit
`ee3d4bd9bd741f197de231b9fd9b002e9db55577` (MIT). The original word/line
range anchors reversed drags; Plico resolves visual lines through retained
native/Parley layouts and keeps password offsets in masked display coordinates.
The existing MIT notice applies. No MyGO source dependency is introduced.
