# Plico recipe authoring

Private repository tooling: TypeScript imports and helpers generate a versioned
JSON plan for a statically compiled Rust runner. Bun 1.4.4 manages dependencies,
executes TypeScript and runs tests; `bun run check` separately checks types.

The application runner lives in the separate Plicowarden checkout. From that
repository's root:

```sh
python3 scripts/run-recipes.py
python3 scripts/run-recipes.py --no-build --semantic
```

The current command targets Plicowarden's synthetic fixtures. No C# or embedded JS
engine is needed. [Protocol, examples, reports and limits](../../docs/recipes.md).

```sh
bun install --cwd tools/recipes --frozen-lockfile
bun run --cwd tools/recipes check
bun run --cwd tools/recipes test
```
