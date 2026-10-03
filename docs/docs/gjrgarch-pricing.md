# GJR-GARCH model and European pricing

The shared Rust core provides a six-parameter calibrated model, a European
Edgeworth approximation and a European Monte Carlo diffusion engine through
Rust, C, Go and Python. See [process and paths](gjrgarch.md) for daily/annual
units and the three variance schemes. Neither engine is a historical estimator
or an exact discrete daily GJR-GARCH recursion.

## Model contract

Parameters, calibration masks and returned arrays use this order:

| Position | Rust / Go | Python | Unit and checked model domain |
| --- | --- | --- | --- |
| 0 | `omega` / `Omega` | `omega` | Daily intercept, strictly positive |
| 1 | `alpha` / `Alpha` | `alpha` | Daily coefficient in `[0, 1]` |
| 2 | `beta` / `Beta` | `beta` | Daily coefficient in `[0, 1]` |
| 3 | `gamma` / `Gamma` | `gamma` | Daily coefficient in `[-1, 1]` |
| 4 | `lambda` / `Lambda` | `lambda_` | Finite innovation displacement |
| 5 | `v0` / `DailyVariance` | `daily_variance` | Initial daily variance, strictly positive |

All inputs must be finite. Coupled checks require `beta + gamma >= 0` and the
supported process domain, including `alpha + gamma >= 0`, representable annual
coefficients and nonnegative residual diffusion variance. These are checked
for construction, parameter updates and calibration trials. No stationarity
restriction is imposed. The process alone supports zero omega and variance;
the calibrated model does not.

- Construction and accepted updates rebuild the model process using
  `FullTruncation`, even if the supplied process uses another scheme.
- The analytic engine retains the live model. Parameter changes and live
  spot/curve notifications invalidate attached option prices.
- `model.process()` returns the current process snapshot. Existing MC engines
  retain their supplied process and scheme: they do not retarget when model
  parameters change. Their retained market handles still remain live.
- Rebuild an MC engine from the current model process to sample newly fitted
  parameters. Model-derived processes use `FullTruncation`.

## Example: approximation, sampled price and live quote

These examples use the same market, one-year call, daily parameters and seed.
The analytic and MC numbers need not agree within the MC standard error.
After attaching MC, the last `npv` call reprices that MC engine.

=== "Python"

    ```python
    from itofin import Settings
    from itofin.instruments import OptionType, VanillaOption
    from itofin.models import GJRGARCHModel
    from itofin.pricingengines import AnalyticGJRGARCHEngine, MCEuropeanGJRGARCHEngine
    from itofin.processes import GJRGARCHProcess
    from itofin.quotes import SimpleQuote
    from itofin.termstructures import FlatForward
    from itofin.time import Date, DayCounter

    today = Date(3, 10, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    dc = DayCounter.actual365_fixed()
    spot = SimpleQuote(100.0)
    risk = FlatForward(today, 0.05, dc)
    dividend = FlatForward(today, 0.02, dc)
    process = GJRGARCHProcess(
        spot, risk, dividend, 0.00016, 0.000002, 0.04, 0.9, 0.06, 0.1,
        days_per_year=252.0, scheme="FullTruncation",
    )
    model = GJRGARCHModel(process)
    option = VanillaOption(OptionType.Call, 100.0, today + 365, settings)
    analytic = AnalyticGJRGARCHEngine(model)
    print("Analytic GJR NPV:", option.price_gjr(analytic))
    mc = MCEuropeanGJRGARCHEngine(
        process, steps_per_year=252, samples=4096, seed=42, antithetic=True,
    )
    print("Monte Carlo GJR NPV:", option.price_mc_gjr(mc))
    print("Monte Carlo standard error:", option.error_estimate())
    print("Daily omega,alpha,beta,gamma,lambda,v0:", model.params())
    spot.set_value(105.0)
    print("After live spot update:", option.npv())
    ```

=== "Go"

    ```go
    --8<-- "sdk/go/examples/gjr_pricing/main.go"
    ```

=== "Rust"

    ```rust
    --8<-- "crates/libitofin/examples/gjr_pricing.rs"
    ```

## Analytic approximation: source parity, not an exact oracle

The engine ports the [pinned QuantLib Edgeworth moment expansion](https://github.com/lballabio/QuantLib/blob/9863b578af0caa4cecabf697196533e84a8308b6/ql/pricingengines/vanilla/analyticgjrgarchengine.cpp).
It supports European plain-vanilla NPV only. Unsupported exercise/payoff types
and requests for missing Greeks fail explicitly.

**Preserved nonstandard dividend convention:** with maturity discounts `Dr`
(risk-free) and `Dq` (dividend), the source uses

```text
call - put = spot - strike * Dr / Dq
```

The call is not multiplied by `Dq`. This is not conventional discounted
put-call parity. Finite negative approximation values or arbitrage-bound
violations are not clamped or silently replaced by another pricer.

| Analytic guard | Behavior |
| --- | --- |
| Spot, strike, discounts | Finite and strictly positive |
| Maturity | Finite positive process time |
| Daily horizon `T` | `round(days_per_year * time)` in `1..=1000` |
| Moment poles | Reject coefficient differences within `64 * f64::EPSILON * max(1, abs(a), abs(b))` |
| Moments/output | Reject zero expected variance, nonpositive computed variance or nonfinite results |
| Work/storage | Direct cubic moment summation, linear temporary storage |

The moment cache includes all six parameters, rounded horizon and daily carry
rate. Spot and discounts are read live. This fixes the pinned source's stale
carry-rate cache behavior: rate/dividend bumps match fresh-engine calculations,
not its stale cached result.

## Monte Carlo configuration and uncertainty

MC discounts the terminal plain-vanilla payoff under the supplied diffusion
process. It preserves `PartialTruncation`, `FullTruncation` or `Reflection`.
European calls/puts are supported, including zero strike. Greeks are unavailable.

| Configuration | Contract |
| --- | --- |
| Grid | Exactly one of fixed `steps` or `steps_per_year` |
| Sampling | Exactly one of fixed `samples` or positive finite absolute tolerance |
| Fixed pseudo-random samples | At least two observations |
| Pseudo-random seed | Nonzero uint32 seed is reproducible; zero means randomized seed |
| Antithetic | One observation averages a path and its negated-draw partner |
| Standard error | Sampling error of observations/pair averages, not model or discretization error |
| Tolerance cap | Defaults to 50,000 observations; exhaustion returns an error |
| Limits | 100,000 steps, 1,000,000 observations, 100,000,000 factor-step evaluations |

Work is `steps * 2 factors * observations`, doubled for antithetic partners.
The requested maximum sample budget is included in the work check. Raising
`max_samples` cannot bypass the step, observation or work ceilings. Tolerance
mode requires a cap at least as large as its initial batch of 1,023 observations.
These errors do not return a successful partial price.

Rust also supports the low-discrepancy RNG policy with fixed samples only,
without a standard-error estimate. Python/Go expose pseudo-random MC.
Brownian bridge and control variates are not exposed by these GJR engines.
Pseudo-random MC seed zero follows the existing randomized-seed convention; the separate
[stateless batch-path API](gjrgarch.md#seeded-paths) rejects seed zero instead.

### Keep approximation bias separate from sampling error

For the native matrix case `lambda=0.2`, 180-day maturity and strike 50:

| Quantity | Native value |
| --- | --- |
| Analytic approximation | `4.54912735976` |
| Diffusion MC estimate | `5.60851935058` |
| Difference / MC standard error | About `53.13` |

This is approximation bias, not evidence of agreement within sampling error.
The original 36-case cached-value tests retain their separate absolute `0.15`
acceptance bands unchanged. Independent native price/error fixtures are checked
separately. The constant-variance diffusion limit is independently checked
against Black pricing within four MC standard errors; that limit does not
make the general analytic approximation exact.

## Calibration and rollback

Calibration reuses `HestonModelHelper` Black-volatility quotes with the analytic
GJR engine, the existing optimizers, optional nonnegative weights and a
six-position fixed-parameter mask. Optional constraints are intersected with
the checked model constraint, not substituted for it. A `true` mask position
holds that parameter fixed.

- Failed optimization or final helper evaluation restores the exact previous
  six parameters and retained process snapshot, plus the previous end-criteria
  status, residual vector and function-evaluation count.
- Rollback does not reconstruct the saved process from current market inputs.
  It therefore works even if a quote/curve becomes invalid during fitting.
- Market values are not rolled back. Pricing still reports the invalid market
  until it is repaired, and observers are notified after model restoration.

The original 21-helper DAX calibration retains its nonflat risk-free curve and
percent-volatility SSE acceptance criterion `<= 15`. The checked Rust fit
reproduces native SSE about `7.8522` with 604 function evaluations. These are
fixture results, not a guarantee for another calibration dataset.

Python/Go `HestonModelHelper` constructors currently accept flat-rate inputs,
not arbitrary curve handles. The Python DAX acceptance test uses a maturity-specific
effective-rate adapter, `rate = -log(discount_at_maturity) / time`, to reproduce
the nonflat curve's discount for each helper. The GJR process/engine itself
retains the actual nonflat curve. Go checks independent flat-helper fits, not
the full nonflat DAX case. This adapter is not a general curve-helper API.

## API reference

- [GJR model](api/models.md#itofin.models.GJRGARCHModel)
- [Analytic engine](api/pricingengines.md#itofin.pricingengines.AnalyticGJRGARCHEngine)
- [Monte Carlo engine](api/pricingengines.md#itofin.pricingengines.MCEuropeanGJRGARCHEngine)
