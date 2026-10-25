# Cash-digital BlackCalculator

The additive cash-or-nothing factory returns the existing immutable
`BlackCalculator`, with all its existing scalar queries. The original vanilla
constructor and its numerical behavior are unchanged. This is a standalone Black
1976 calculator, not an instrument, pricing engine or live market-data observer.

## Construction

| Language | Factory |
| --- | --- |
| Python | `BlackCalculator.cash_digital(option_type, strike, cash_payoff, forward, std_dev, discount)` |
| Go | `session.NewCashDigitalBlackCalculator(option, strike, cashPayoff, forward, stdDev, discount)` |
| C | `itofin_black_calculator_cash_digital_new` |

All numeric inputs must be finite. Strike is nonnegative; forward and discount
are positive. Cash payoff can be positive, zero or negative, matching the existing
European cash-digital instrument factory. Inputs are copied; no payoff object,
curve, quote or settings object is retained.

**The new factory requires `std_dev > 2^-52`**, where `2^-52` is Rust's
`f64::EPSILON`. `std_dev` means volatility times the square root of time.
The restriction excludes the existing core's degenerate-volatility shortcut,
which returns vanilla-shaped sensitivities rather than valid digital Greeks.
Zero, tiny deviations up to and including that exact threshold, NaN and infinity
are rejected. This is a restriction on the new factory, not on vanilla callers.
It is not a promise of uniformly accurate Greeks immediately above the threshold.

```python
from itofin.instruments import OptionType
from itofin.pricingengines import BlackCalculator

calculator = BlackCalculator.cash_digital(OptionType.Call, 100., 7., 110., .3, .94)
price = calculator.value()
forward_delta = calculator.delta_forward()
vega = calculator.vega(2.)
```

Go uses the same parameter order. Scalar methods return `(float64, error)`;
close both calculator and session explicitly. C option values remain 0 for call
and 1 for put. An unknown option value is an error, and C writes its result or
handle output only on success. See the
[general calculator contract](black-calculator-bindings.md) for query arguments,
spot/maturity validation, expiry theta shortcut and C/Go lifecycle.

## Values and preserved numerical limits

For positive nondegenerate volatility and positive strike, let
`d2 = log(forward / strike) / std_dev - std_dev / 2`. A call's value is
`discount * cash_payoff * N(d2)` and a put's is
`discount * cash_payoff * N(-d2)`. The exposed `alpha` is zero and `beta` is that
option-dependent probability. The value formula therefore uses `cash_payoff`,
not the strike, as the coefficient multiplying `beta`.

- `itm_cash_probability` and `itm_asset_probability` still return `N(d2)` and
  `N(d1)` regardless of option sign. For a put these are not its exercise
  probabilities. They do not change with signed or zero cash payouts.
- Native finite elasticity sentinels `f64::MAX` and `f64::MIN` remain available.
  They are sentinel values, **not ordinary percentage sensitivities**. In
  particular negative prices can select a sentinel even when a mathematical
  elasticity ratio exists. Zero-cash calculators ordinarily return zero
  elasticity when the delta is also zero.
- Zero strike remains constructible. A call's finite value is discounted cash;
  the inherited core can return NaN for gamma, strike sensitivity, vega and theta
  at positive deviation. Facades reject those results instead of repairing them.
- All nonfinite query results, including overflow from finite inputs, are errors.
  Python raises `ItofinError`; Go returns an error; C leaves its output untouched.
  Failure does not mutate the calculator or invalidate subsequent finite reads.
- The factory does not repair inherited small-deviation cancellation or add
  unsupported asset-digital/gap visitors, `vanna`, `volga` or `strike_gamma`.

## Independent checks

The compact fixture `sdk/go/testdata/cash_digital_black_calculator.tsv` records
78 independent QuantLib 1.43 scenarios over 17 public scalar queries: signed and
zero cash, call/put, moneyness, zero strike and small deviations. The 30 rows at or
below `f64::EPSILON` remain native diagnostics and must be rejected by the new
factory. Native nonfinite result classes become facade errors. Independent
analytic formulas also cover all 17 queries in the ordinary positive-strike
regime. This is bounded scalar coverage, not full engine or QuantLib parity.

Regenerate with `scripts/generate_cash_digital_black_calculator_oracle.py` using
QuantLib 1.43. The fixture records the compiled native package hash. The pinned
C++ source reference is `9863b578af0caa4cecabf697196533e84a8308b6`; it is a separate
reference, not a claim that the wheel was built from that source.

Runnable examples:
[`example/python/cash_digital_black_calculator.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/cash_digital_black_calculator.py)
and
[`sdk/go/examples/cash_digital_black_calculator/main.go`](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/cash_digital_black_calculator/main.go).
