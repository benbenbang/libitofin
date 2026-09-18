# Development

[Project index](../README.md)

Requires the toolchain pinned in [`rust-toolchain.toml`](../rust-toolchain.toml)
(Rust 1.96.0, edition 2024); plain `cargo` picks it up automatically.

```sh
cargo build                          # whole workspace
cargo build -p libitofin             # core crate only

cargo test                           # the porting oracle
cargo test -p libitofin patterns::   # one module

cargo fmt
cargo clippy --all-targets
```

Run the repository hooks with `prek`. They validate Rust, Python, Go/C,
and release contracts; normal commits also validate the commit message:

```sh
prek run --all-files
```

## Project layout

```
crates/libitofin/       the core library - FFI-agnostic, idiomatic Rust
crates/libitofin-ffi/   extern "C" + cbindgen -> C header + libitofin_ffi
crates/itofin-py/       PyO3 + maturin -> the `itofin` package       (on PyPI)
sdk/go/                Go 1.27.1 cgo package with explicit sessions
sdk/node/              Node SDK placeholder (not implemented)
QuantLib/               reference C++ tree + test oracle           (git-ignored symlink)
```

The `QuantLib/` entry is a **git-ignored local symlink**, not committed - point
it at a QuantLib checkout to have the reference source and test-suite oracle
available locally: `ln -s /path/to/QuantLib QuantLib`.

See the [docs development guide](../docs/README.md) for MkDocs commands and
the [Go SDK guide](../sdk/go/README.md) for native build validation.
