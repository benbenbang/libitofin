# Plain-vanilla BlackCalculator bindings

`BlackCalculator` is an immutable standalone Black 1976 calculator for a call or
put. It copies `option_type`, `strike`, `forward`, `std_dev` and `discount`. It is
not an instrument, pricing engine or volatility/curve observer. No payoff-object
visitor or digital payoff is exposed by this interface.

## Construction and queries

All constructor numbers must be finite. Strike and standard deviation are
nonnegative; forward and discount are strictly positive. `std_dev` means
volatility multiplied by the square root of time, not volatility alone.

```python
from itofin.instruments import OptionType
from itofin.pricingengines import BlackCalculator

calculator = BlackCalculator(OptionType.Call, 100.0, 110.0, 0.3, 0.94)
price = calculator.value()
delta = calculator.delta(97.0)
vega = calculator.vega(2.0)
```

Go uses `session.NewBlackCalculator(itofin.Call, 100, 110, .3, .94)` and methods
such as `Value()`, `Delta(97)` and `Vega(2)`, each returning `(float64, error)`.
Close the calculator and session explicitly. C uses
`itofin_black_calculator_new` and `itofin_black_calculator_<query>` with a typed
context handle. Option discriminants remain 0 for call and 1 for put.

| Queries | Arguments |
| --- | --- |
| `value`, `alpha`, `beta` | None |
| `delta_forward`, `gamma_forward`, `elasticity_forward` | None |
| `delta`, `gamma`, `elasticity` | Positive finite `spot` |
| `theta`, `theta_per_day` | `spot`, finite nonnegative `maturity` |
| `vega`, `rho`, `dividend_rho` | Finite nonnegative `maturity` |
| `itm_cash_probability`, `itm_asset_probability`, `strike_sensitivity` | None |

Theta is per year; `theta_per_day` divides it by 365. Probabilities are the native
`N(d2)` and `N(d1)` regardless of call/put sign, not put exercise probabilities.
`alpha` and `beta` are the native coefficients in
`discount * (forward * alpha + strike * beta)`.

## Errors and preserved native edge behavior

- Every nonfinite computed scalar is an error in C, Go and Python. C writes its
  output only on success. Python raises `ItofinError`; Go propagates the C error.
  The policy includes overflow from otherwise finite inputs.
- Zero strike remains a valid constructor input. The existing core may produce
  NaN for gamma, vega, strike sensitivity and theta at positive deviation,
  including their forward/per-day variants. These queries
  error rather than returning NaN or silently substituting repaired formulas;
  price and probabilities remain available when finite.
- Zero/tiny deviation retains the Rust core's plain-vanilla moneyness ladder.
  In particular an out-of-the-money zero-volatility put has zero delta and strike
  sensitivity. This intentionally differs from the documented QuantLib C++
  `alpha >= 0` branch artifact. No core mathematics changed here.
- Very small positive deviation near the money can suffer inherited cancellation
  in some Greeks and elasticities. Direct core comparison confirms these are not
  binding regressions; independent high-precision formulas and the QuantLib
  runtime can differ. This binding does not promise uniform high-accuracy parity
  in that regime and does not repair or hide the discrepancy. A strict comparison
  against the QuantLib 1.43 runtime retained 18 failed tiny-volatility scalar
  comparisons. Tolerances were not increased and no core repair was performed.
- Native finite elasticity sentinels `f64::MAX` and `f64::MIN` are preserved.
- Native theta's expiry shortcut is preserved: when maturity is close to zero,
  theta is zero without consulting spot. At other maturities spot is validated
  through the native delta/gamma calls. No additional facade domain is imposed.
- Constructor and query failures do not mutate this immutable calculator.

C contexts/handles remain confined to the creating thread. Go serializes calls on
its session worker and rejects closed or missing receivers. Python owns the
copied scalar core object and permits concurrent immutable reads. This feature
retains no external dependencies. See the [binding contract](https://github.com/benbenbang/libitofin/blob/main/docs/go-binding-contract.md)
for C/Go lifecycle rules.

This is bounded plain-vanilla scalar coverage, not complete QuantLib payoff or
pricing-engine parity. Analytical tests are independently derived lognormal
payoff formulas, not copied QuantLib fixtures. Existing core divergences are
preserved and explicitly tested.
