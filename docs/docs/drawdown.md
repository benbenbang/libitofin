# Maximum drawdown

Drawdown is a **nonnegative fractional loss from the running peak** of an
ordered positive portfolio equity/NAV path. It is not a signed return,
an annualized statistic, or a loss percentile.

For each observation `i`, let `Pᵢ = max(NAV₀, ..., NAVᵢ)`. The result is
`maxᵢ (Pᵢ - NAVᵢ) / Pᵢ`, with zero-based peak and trough indices.

| Contract | Behavior |
| --- | --- |
| Inputs | Ordered, finite, strictly positive NAV values |
| Empty input | Error |
| One observation or no decline | `drawdown = 0`, indices `(0, 0)` |
| Equal running peaks | Retain the earliest peak index |
| Equal computed maximum losses | Retain the first winning trough |
| Invalid value anywhere | Error, including after an earlier winning trough |
| Result | Owned snapshot, no session, handles or destruction |

`[100, 120, 90, 130]` loses `(120 - 90) / 120 = 0.25`, or **25%**,
from peak index 1 to trough index 2. Later recovery does not erase that loss.
`[100, 80, 100, 70]` keeps peak index 0, even after recovery to the same peak.
`[100, 50, 200, 100]` retains the first 50% loss, indices `(0, 1)`.

!!! warning "Preserve observation order"
    Unordered return samples cannot identify drawdown. Supply an ordered equity
    or NAV path, not the returns themselves. This utility does not reconstruct
    NAVs, adjust cash flows, assume an initial investment, or sort samples.

## Runnable examples

=== "Python"

    ```python
    --8<-- "example/python/drawdown.py"
    ```

=== "Go"

    ```go
    --8<-- "sdk/go/examples/drawdown/main.go"
    ```

From a native-build checkout, run `python example/python/drawdown.py` or
`cd sdk/go && go run ./examples/drawdown`.

## Shared APIs and numerical limits

- Rust: `libitofin::math::statistics::{maximum_drawdown, DrawdownResult}`.
- Python: `itofin.statistics.maximum_drawdown` and immutable `DrawdownResult`.
- Go: `itofin.MaximumDrawdown` and `DrawdownResult`.
- C: `itofin_maximum_drawdown`, writing one caller-owned `ItofinDrawdownResult`
  with `double drawdown` and `size_t peak_index, trough_index`. Errors leave the
  entire result unchanged. Follow the generated header's pointer contract.

The one-pass computation uses constant extra space and `(peak - NAV) / peak`.
Strictly positive finite inputs make this subtraction safe from overflow, and
preserve tiny representable losses better than subtracting a ratio from one.
Floating-point comparisons define loss ties. Extremely small positive troughs
relative to peaks may round the fractional loss to **1**; that is not proof
that a supplied positive NAV reached zero. No weights or annualization apply.

## Independent fixtures

The [hand-calculated fixture](https://github.com/benbenbang/libitofin/blob/main/sdk/go/testdata/drawdown.json)
covers increase/decrease, recovery, repeated peaks/troughs, equal maximum
losses and multiple drawdowns. Python independently enumerates every ordered
peak/trough pair with exact rational arithmetic; Rust, C/C++ and Go validate
the same contract, errors and snapshot behavior.
