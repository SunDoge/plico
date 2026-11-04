# Wabou gallery ports

Source: Wabou, revision `739624e09693326b283ffaabfc3cc74cc7651a84`.
The Apache-2.0 license is preserved in `WABOU-LICENSE`.

This snapshot's source paths:

- `apps/gallery/ui/pages/data-table.tsx`: the six project records, filter,
  column sorting, selection, clear-selection and no-results behavior.
- `apps/gallery/ui/pages/tree-view.tsx`: workspace tree, expanded/selected state,
  disabled target directory, and a deployment tree with app-rendered badges.

The TSX declarations are rewritten into ordinary Plico components. The project
fixture keeps Wabou's IDs, names, owners and scores. Workspace names are adapted
for Plico. Runtime state uses component-scoped `persisted_state`; sorting and
filtering are a small Rust sample, not a port of TanStack Table.

Table headers stay outside the scrolling row viewport. The tree is composed of
native Plico buttons and uses their Tab/Enter/Space support; it is not yet an
ARIA tree with arrow-key roving focus, nor a new low-level backend node kind.
Both pages support English/Chinese and the gallery's normal theme controls.
There is no path dependency, source symlink, or build-time access to Wabou.

PNG captures are local `reports/` artifacts. Rebuild them with:

```sh
cargo run -p gallery -- --scenario apps/gallery/scenarios/data-table.json --capture reports/gallery-data-table.png
cargo run -p gallery -- --scenario apps/gallery/scenarios/tree-view.json --capture reports/gallery-tree-view.png
```
