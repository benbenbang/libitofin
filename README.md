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
| Standalone currencies | [Constructor and fields](docs/docs/currency-bindings.md) · [Presets including TWD](docs/docs/currency-presets.md) |
| FX delta helpers | [Black delta conventions](docs/docs/black-delta-bindings.md) |
| Vanilla pricing and risk | [BlackCalculator](docs/docs/black-calculator-bindings.md) · [Bond risk analytics](docs/docs/bond-analytics.md) · [Coupon BPS and ATM rate](docs/docs/bond-coupon-analytics.md) |
| European digital and gap options | [Cash-or-nothing analytic pricing](docs/docs/cash-digital-bindings.md) · [Asset-or-nothing FD pricing](docs/docs/asset-digital-bindings.md) · [Signed gap FD pricing](docs/docs/gap-option-bindings.md) |
| European percentage-strike options | [Dimensionless-ratio FD pricing](docs/docs/percentage-strike-bindings.md) |
| Bond price and principal schedules | [Price-fed ATM](docs/docs/bond-price-atm-bindings.md) · [Amortizing fixed bonds](docs/docs/amortizing-fixed-bonds.md) · [Amortizing Ibor bonds](docs/docs/amortizing-floating-bond-bindings.md) |
| Retained instruments | [Par asset swaps](docs/docs/asset-swap-bindings.md) · [Fixed-date FX forwards](docs/docs/fx-forward-bindings.md) · [Constant-notional Ibor Xccy](docs/docs/xccy-basis-bindings.md) |
| Standalone Gaussian processes | [Constant-parameter GSR](docs/docs/gsr-process-bindings.md) · [Hull-White forward](docs/docs/hw-forward-process-bindings.md) · [MF state](docs/docs/mf-state-process-bindings.md) |
| Worked pricing walkthrough | [Python, Rust, and Go](https://benbenbang.github.io/libitofin/getting-started/) |
| Releases, coverage, and remaining work | [Status and scope](wiki/status.md) · [Issues](https://github.com/benbenbang/libitofin/issues) |
| Build, test, and repository layout | [Development](wiki/development.md) |
| Architecture and porting principles | [Design](wiki/design.md) |
| Deliberate differences from QuantLib | [Compatibility notes](wiki/compatibility.md) |

Process bindings are not fitted models. Coupon BPS/ATM covers selected
ordinary-bond helpers, not the wider `BondFunctions` overload set. Asset/gap FD
pricing retains documented grid limitations. Amortizing constructors cover only
positive, nonincreasing balances.

The tracked `wiki/` pages hold detailed guides; the
[MkDocs site](https://benbenbang.github.io/libitofin/) provides language tabs and
API navigation.

## License

[BSD-3-Clause](LICENSE), the same license as QuantLib.
