# Lib-Itô-Fin

[![Crates.io](https://img.shields.io/crates/v/libitofin)](https://crates.io/crates/libitofin)
[![PyPI](https://img.shields.io/pypi/v/itofin)](https://pypi.org/project/itofin/)
[![docs.rs](https://img.shields.io/docsrs/libitofin)](https://docs.rs/libitofin)
[![Python](https://img.shields.io/pypi/pyversions/itofin)](https://pypi.org/project/itofin/)
[![License: BSD-3-Clause](https://img.shields.io/crates/l/libitofin)](LICENSE)

A Rust port of [QuantLib](https://github.com/lballabio/QuantLib) for pricing,
risk, and numerical methods, with Python and Go bindings over the same core.
Pre-1.0: APIs may change. See [status and scope](wiki/status.md) and the
[release history](https://github.com/benbenbang/libitofin/releases).

## Install

### Rust

```sh
cargo add libitofin
```

[Rust guide and examples](wiki/rust.md) · [API reference](https://docs.rs/libitofin)

### Python

Requires Python 3.10 or newer.

```sh
pip install itofin
```

[Python guide and examples](wiki/python.md) · [API reference](https://benbenbang.github.io/libitofin/api/core/)

### Go

Requires Go 1.27.1, cgo, a C compiler, and the matching native release package.
Follow the [native installation guide](https://benbenbang.github.io/libitofin/go/#install-in-an-application)
first, then add the matching module version:

```sh
go get github.com/benbenbang/libitofin/sdk/go@v0.24.0
```

Use `-tags itofin_external` for builds, runs, tests, and vet outside this checkout.
[Go guide and examples](wiki/go.md) · [API reference](https://pkg.go.dev/github.com/benbenbang/libitofin/sdk/go)

## Documentation

| Topic | Start here |
|-------|------------|
| Language guides and examples | [Rust](wiki/rust.md) · [Python](wiki/python.md) · [Go](wiki/go.md) |
| Worked pricing walkthrough | [Python, Rust, and Go](https://benbenbang.github.io/libitofin/getting-started/) |
| Releases, coverage, and remaining work | [Status and scope](wiki/status.md) · [Issues](https://github.com/benbenbang/libitofin/issues) |
| Build, test, and repository layout | [Development](wiki/development.md) |
| Architecture and porting principles | [Design](wiki/design.md) |
| Deliberate differences from QuantLib | [Compatibility notes](wiki/compatibility.md) |

The tracked `wiki/` pages hold detailed guides; the
[MkDocs site](https://benbenbang.github.io/libitofin/) provides language tabs and
API navigation.

## License

[BSD-3-Clause](LICENSE), the same license as QuantLib.
