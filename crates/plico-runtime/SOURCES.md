# Sources and design references

Text selection interactions in `src/selection.rs` were adapted from the unit-based
anchor/drag design in MyGO's
[`ui/textselection.go`](https://github.com/egoist/mygo/blob/7e8688284e4b1fc752169ed9db5f51a5f213a2e6/ui/textselection.go)
(MIT). Character/word/paragraph units retain their initial range during reverse
drags. Plico uses explicit retained paragraph IDs, UTF-8 offsets, Rust Unicode
segmentation and its injected clock rather than MyGO's Go editor/runtime objects.
The original MIT notice is retained in `licenses/mygo-MIT.txt`.
No MyGO checkout is needed to build or run Plico.
