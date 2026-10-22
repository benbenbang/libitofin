# Black delta calculator bindings

Standalone immutable FX delta and strike conversion wraps the adopted native
`BlackDeltaCalculator`. This adds no pricing engine, calibration or numerical
implementation. Create a new calculator to change option or delta convention;
native setters are deliberately not exposed.

## Conventions

The existing option mapping is unchanged: C/Go/Python use `0 = Call`, `1 = Put`.
These are not the native Rust option's signed discriminants.

| Delta value | C suffix `ITOFIN_DELTA_` | Go | Python `DeltaType` |
| --- | --- | --- | --- |
| 0 | `SPOT` | `DeltaSpot` | `Spot` |
| 1 | `FWD` | `DeltaFwd` | `Fwd` |
| 2 | `PA_SPOT` | `DeltaPaSpot` | `PaSpot` |
| 3 | `PA_FWD` | `DeltaPaFwd` | `PaFwd` |

| ATM value | C suffix `ITOFIN_ATM_` | Go | Python `AtmType` |
| --- | --- | --- | --- |
| 0 | `NULL` | `AtmNull` | `Null` |
| 1 | `SPOT` | `AtmSpot` | `Spot` |
| 2 | `FWD` | `AtmFwd` | `Fwd` |
| 3 | `DELTA_NEUTRAL` | `AtmDeltaNeutral` | `DeltaNeutral` |
| 4 | `VEGA_MAX` | `AtmVegaMax` | `VegaMax` |
| 5 | `GAMMA_MAX` | `AtmGammaMax` | `GammaMax` |
| 6 | `PUT_CALL_50` | `AtmPutCall50` | `PutCall50` |

C values are explicit stable integer constants, with unknown values rejected.
`Null` cannot calculate an ATM strike. `PutCall50` requires **unadjusted Fwd**,
not `PaFwd`. Gamma/vega maxima use the native positive exponential even for
premium-adjusted conventions. Python enums live in `itofin.quotes`.

## Python

```python
from itofin.instruments import OptionType
from itofin.pricingengines import BlackDeltaCalculator
from itofin.quotes import DeltaType, AtmType

calc = BlackDeltaCalculator(
    OptionType.Call, DeltaType.PaFwd, 1.421, .997306, .992266, .1180654
)
delta = calc.delta_from_strike(1.6)
strike = calc.strike_from_delta(.15)
atm = calc.atm_strike(AtmType.DeltaNeutral)
```

## Go and C

Go creates the same owned immutable object using
`session.NewBlackDeltaCalculator(Call, DeltaPaFwd, spot, domesticDF, foreignDF, stdDev)`.
Methods are `DeltaFromStrike`, `StrikeFromDelta` and `AtmStrike`. Close the
calculator and its session explicitly. Session calls serialize on their native
worker thread; handles cannot be used with another session.

C uses `itofin_black_delta_new`, `itofin_black_delta_delta_from_strike`,
`itofin_black_delta_strike_from_delta` and `itofin_black_delta_atm_strike`.
Release the handle with `itofin_handle_release`. Scalar/handle outputs are
written only on success. Context thread confinement and the
[C caller contract](https://github.com/benbenbang/libitofin/blob/main/docs/go-binding-contract.md) still apply.

## Domains and ambiguity

- Spot and domestic/foreign discount factors must be finite and positive.
- `std_dev` is finite nonnegative **volatility times square root of time**.
  There is no separate time input. Unrepresentable derived forwards or strikes
  return errors rather than nonfinite results.
- `delta_from_strike` accepts finite nonnegative strikes and zero deviation.
  Premium-adjusted put deltas can be less than minus one and are not clamped.
- Inversion requires positive deviation and an interior delta with the option
  sign, scaled by foreign discount for spot conventions. Boundary, wrong-sign,
  infeasible and numerically unresolved deltas are errors, as in the core.
- Premium-adjusted calls have two strike roots. Inversion deliberately selects
  the **high-strike branch**, so strike-delta-strike is not an identity for a
  low-strike input. Delta-strike-delta remains the appropriate roundtrip.

The binding tests reuse the core's eight compiled QuantLib 1.43 fixture rows
without changing their values or tolerances. They are retained reference data,
not a claim that QuantLib was freshly run by these binding tests.
