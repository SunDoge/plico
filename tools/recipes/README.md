# Plico recipe authoring

Private repository tooling: TypeScript imports and helpers generate a versioned
JSON plan for a statically compiled Rust runner. Node >=22.18 is needed to execute
erasable TS; `npm run check` separately checks types.

The application runner lives in the separate Plicowarden checkout. From that
repository's root:

```sh
python3 scripts/run-recipes.py
python3 scripts/run-recipes.py --no-build --semantic
```

The current command targets Plicowarden's synthetic fixtures. No C# or embedded JS
engine is needed. [Protocol, examples, reports and limits](../../docs/recipes.md).

```sh
npm ci --prefix tools/recipes
npm --prefix tools/recipes run check
npm --prefix tools/recipes test
```
