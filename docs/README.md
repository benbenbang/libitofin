# itofin docs site

The `itofin` Python, Rust, and Go documentation, built with [MkDocs](https://www.mkdocs.org/) +
[Material](https://squidfunk.github.io/mkdocs-material/) +
[mkdocstrings](https://mkdocstrings.github.io/). mkdocstrings reads the hand-written `.pyi`
stubs in `crates/itofin-py/python/itofin/` **statically** (via griffe) - no compiled
extension or maturin build is needed to build the docs.

The Rust core (`libitofin`) API is not duplicated here; it is published on
[docs.rs/libitofin](https://docs.rs/libitofin).

Go setup and ownership guidance live in `docs/go.md`. The Getting started page
includes the runnable Go European option source; the Go guide includes the
portfolio example. Full Go API docs are linked on pkg.go.dev.

## Develop

```bash
cd docs
uv sync
uv run mkdocs serve      # live preview at http://127.0.0.1:8000
uv run mkdocs build --strict
```

CI (`.github/workflows/docs.yml`) builds `--strict` on every PR and publishes to GitHub
Pages on push to `main`. Pages must be set to the "GitHub Actions" source in the repo
settings for the deploy job to publish.
