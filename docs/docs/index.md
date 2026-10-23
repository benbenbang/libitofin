# itofin

For interrupted publication, see [release recovery](release-recovery.md).

[`libitofin`](https://crates.io/crates/libitofin) is a ground-up port of
[QuantLib](https://www.quantlib.org/) into idiomatic Rust, with Python and Go
bindings. All three language surfaces share the Rust numerical core.

- **Fidelity in numerics, usability at the boundary.** QuantLib is the oracle for every
  number; the Python API adds ergonomic conveniences (keyword constructors, `price(engine)`)
  without changing the math.
- **Typed and introspectable.** Every class ships hand-written `.pyi` stubs with Google-style
  docstrings, so editors autocomplete and this site renders the full signature.

Go and Python expose fixed/zero/floating bond instruments, retained discounting
engines and [yield/Z-spread analytics](bond-analytics.md):
[bond binding guide](bond-bindings.md). Existing bootstrap helpers remain
separate. [Callable bonds, Hull-White tree and single-European Black pricing/IV](callable-bond-bindings.md)
share the retained Bond interface. OAS and effective risk measures remain
separate binding work.

## Standalone binding helpers

- [Reusable currencies and stored fields](currency-bindings.md), plus
  [named currency presets including TWD](currency-presets.md).
- [Black FX delta conventions](black-delta-bindings.md): standalone
  delta/strike and ATM conversions, not an FX option pricing engine.

## Install

=== "Python"

    ```bash
    pip install itofin
    ```

=== "Rust"

    ```bash
    cargo add libitofin
    ```

=== "Go"

    Install the matching native library first, following the [Go SDK guide](go.md).
    Then add the module to your application:

    ```sh
    go get github.com/benbenbang/libitofin/sdk/go@v0.24.0
    ```

    Go requires cgo, a C compiler, and the `itofin_external` build tag for
    applications outside this repository.

The [Getting started](getting-started.md) page has runnable pricing examples
in Python, Rust, and Go. The [project index](https://github.com/benbenbang/libitofin#readme)
links to the language guides and project documentation.

## Where the docs live

| Surface | Where |
|---------|-------|
| Python API reference | This site (see the **Python API** section) |
| Rust API reference | [docs.rs/libitofin](https://docs.rs/libitofin) - see [Rust API](rust.md) |
| Go SDK | [Installation, sessions, and examples](go.md), plus [API reference](https://pkg.go.dev/github.com/benbenbang/libitofin/sdk/go) |
| Worked examples | [`example/python`](https://github.com/benbenbang/libitofin/tree/main/example/python), [`sdk/go/examples`](https://github.com/benbenbang/libitofin/tree/main/sdk/go/examples), and [`crates/libitofin/examples`](https://github.com/benbenbang/libitofin/tree/main/crates/libitofin/examples) |

## Reviewed Rust additions

- [Priority adoption scope and limits](priority-adoption.md): 18 retained High
  rows, with 10 excluded because existing issue acceptance already owns them.
- Rust-core only: no new Python, C or Go facades, and no release implied.

## Project guides

- Language guides: [Rust](https://github.com/benbenbang/libitofin/blob/main/wiki/rust.md),
  [Python](https://github.com/benbenbang/libitofin/blob/main/wiki/python.md),
  [Go](https://github.com/benbenbang/libitofin/blob/main/wiki/go.md).
- [Status and scope](https://github.com/benbenbang/libitofin/blob/main/wiki/status.md)
- [Development](https://github.com/benbenbang/libitofin/blob/main/wiki/development.md)
- [Design](https://github.com/benbenbang/libitofin/blob/main/wiki/design.md)
- [QuantLib compatibility](https://github.com/benbenbang/libitofin/blob/main/wiki/compatibility.md)

- [Small LOW Rust additions](low-small-additions.md): additive helpers, immutable
  legacy components, independent references and deferred scope.

- [Bond Z-spread analytics](bond-zspread.md): live-curve clean/dirty quotes and
  checked spread inversion with settlement and current-notional conventions.

- [Exchange-rate chaining](exchange-rate-chaining.md): ordered direct/derived
  conversions, currency orientations and checked numerical boundaries.

- [Bond forwards](bond-forward.md): retained live bonds, explicit income/financing curves and checked delivery quotes.

- [Implied volatility term structure](implied-vol-term-structure.md): fixed-reference forward variance, live source handles and explicit extrapolation.

- [Black-Scholes theta](black-scholes-theta.md): current-market theta from supplied Greeks with checked source-order arithmetic.

- [FX forwards](fx-forwards.md): explicit settlement-normalized currency-tagged valuation and snapshot spot inputs.

- [Hull-White forward process](hullwhite-forward-process.md): live yield curves, forward-measure transitions and checked stable limiting formulas.

- [Quanto term structure](quanto-term-structure.md): linked standalone quanto dividend yields with native numeric-time conventions.

- [ABCD math and volatility](abcd.md): coefficient validation, volatility shape and integrated variance/covariance.

- [Householder utilities](householder.md): checked reflections, orthogonal matrices and explicit native application conventions.

- [Zero-coupon bonds](zero-coupon-bonds.md): single-redemption contract and payment dates.

- [Floating-rate bonds](floating-rate-bonds.md): Ibor coupons, gearing, caps/floors and ex-coupon schedules.

- [Par asset swaps](asset-swaps.md): bullet-bond dirty upfront and floating fair spread.
- [Asset swap bindings](asset-swap-bindings.md): retained plain bullet bonds, live Ibor/discount curves and par swap analytics.

- [Cross-currency basis swaps](cross-currency-basis-swaps.md): plain non-resettable Ibor legs and explicit FX valuation.

- [MF state process](mf-state-process.md): standalone Gaussian state law and checked piecewise variance.

- [Asset-or-nothing payoff](asset-or-nothing-payoff.md): strict digital asset payment.

- [Gap payoff](gap-payoff.md): separate trigger and payment strikes.

- [Cap/floor optionlets](capfloor-optionlets.md): shared coupon extraction and pricing decomposition.

- [Constant GSR process](gsr-process.md): exact Gaussian forward-measure dynamics and checked constant grids.

- [Plain float-float swaps](float-float-swaps.md): two-Ibor-leg signed valuation and fair spreads.

- [OIS payer and receiver](ois-side.md): explicit swap side with the existing payer default.

- [Bachelier asset ITM](normal-itm.md): normal-model helper and its zero-volatility convention.

- [Percentage-strike payoff](percentage-strike-payoff.md): dimensionless moneyness and exact multiplication order.

- [Floating-type payoff](floating-type-payoff.md): a supplied strike at each scalar evaluation.

- [Callable zero-coupon bonds](callable-zero-coupon-bonds.md): hard calls/puts and Hull-White lattice quotes.

- [Black callable bonds](black-callable-bonds.md).

- [Callable implied volatility](callable-implied-volatility.md).

- [Callable OAS](callable-oas.md).

- [Callable bond sensitivities](callable-bond-sensitivities.md).

- [Bare-factor G2 process](g2-process.md).
- [Constant-parameter GSR process](gsr-process-bindings.md), with mutable forward-measure horizon.

[Bond forward bindings](bond-forward-bindings.md) retain live plain fixed/zero bonds and explicit financing/income curves.

[Plain-vanilla BlackCalculator](black-calculator-bindings.md) exposes scalar pricing and Greeks with documented inherited numerical limits.
[Bond risk analytics](bond-analytics.md) covers duration, convexity and signed yield sensitivities.
[FX forward bindings](fx-forward-bindings.md) retain live discount curves with immutable spot and explicit contractual dates.
