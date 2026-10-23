# Bond yield and Z-spread analytics

Go and Python expose the existing Rust `BondFunctions` and `CashFlows` routes.
Python methods live on the shared `Bond` base and are inherited by fixed, zero
and floating-rate bonds. There is no separate `BondFunctions` namespace class.
The engine-based `clean_price()` and `dirty_price()` methods are unchanged.

## Python example

An explicit settlement is an actual date, not an evaluation date to advance.
Fixed/zero analytical queries below need neither an engine nor an evaluation date.

```python
from itofin import Settings
from itofin.instruments import ZeroCouponBond
from itofin.termstructures import FlatForward
from itofin.time import Calendar, Compounding, Date, DayCounter, Frequency

settings = Settings()
settlement = Date(15, 1, 2026)
curve = FlatForward(Date(1, 1, 2026), 0.035, DayCounter.actual365_fixed())
bond = ZeroCouponBond(
    2, Calendar.null_calendar(), 1000.0, Date(1, 1, 2028), settings,
)
price = bond.clean_price_at_z_spread(
    curve, 0.007, Compounding.Compounded, Frequency.Semiannual,
    settlement=settlement,
)
spread = bond.z_spread(
    price, curve, Compounding.Compounded, Frequency.Semiannual,
    settlement=settlement,
)
print(f"Clean price per 100: {price:.12f}")
print(f"Recovered Z-spread: {spread:.12f}")
```

Output:

```text
Clean price per 100: 92.115433858272
Recovered Z-spread: 0.007000000000
```

The equivalent checked Go example is
[`sdk/go/examples/bond-analytics/main.go`](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/bond-analytics/main.go).
Go methods return `(float64, error)`; all related handles must belong to one
Session. Python returns `float` or raises the existing typed binding errors.

## Bond routes

| Python on `Bond` | Go on `*Bond` | Inputs beyond the bond |
| --- | --- | --- |
| `clean_price_from_curve` | `CleanPriceFromCurve` | Discount curve |
| `dirty_price_from_curve` | `DirtyPriceFromCurve` | Discount curve |
| `clean_price_from_yield` | `CleanPriceFromYield` | Yield, day counter, compounding, frequency |
| `dirty_price_from_yield` | `DirtyPriceFromYield` | Yield, day counter, compounding, frequency |
| `yield_rate` | `YieldRate` | Quote and yield conventions |
| `clean_price_at_z_spread` | `CleanPriceAtZSpread` | Curve, spread, compounding, frequency |
| `dirty_price_at_z_spread` | `DirtyPriceAtZSpread` | Curve, spread, compounding, frequency |
| `z_spread` | `ZSpread` | Quote, curve, compounding, frequency |

- The price outputs/quote targets in the table above are **per 100 of current settlement notional**.
  Clean quotes exclude accrued interest; dirty quotes include it. Inversions
  accept `price_type=BondPriceType.Clean` by default, or explicit `Dirty`.
- Python `settlement=None` resolves through the existing checked settlement
  method, including lag and issue-date floor. A supplied settlement is passed
  unchanged, with no extra advance or issue-date adjustment.
- Yield pricing uses the core's stepwise cash-flow discount times. It does not
  replace the leg with one terminal discount factor.
- Spread routes use the curve's own day counter. They do not accept a separate
  day counter or change the original curve.
- Nontradable/matured analytical bonds return an error. This is different from
  the existing expired engine quote, which can return zero.

Python optional settlement and solver controls are keyword-only. Go supplies
`BondYieldConfig`, `BondYieldSolveConfig`, `BondZSpreadConfig` or
`BondZSpreadSolveConfig`, with `SettlementDate` and convention fields.

## Leg routes

Python functions live in `itofin.cashflows`; Go methods live on `*Leg`:

| Python function | Go method | Meaning |
| --- | --- | --- |
| `npv_at_z_spread` | `NPVAtZSpread` | Spreaded NPV in currency units |
| `z_spread` | `ZSpread` | Spread repricing a currency NPV target |

The Python argument order is:

```python
npv_at_z_spread(leg, curve, spread, compounding, frequency, settings)
z_spread(leg, target_npv, curve, compounding, frequency, settings)
```

Both functions accept keyword-only `include_settlement_date_flows`,
`settlement_date` and `npv_date`. Inclusion is tri-state: `None` preserves Settings
policy, while explicit false/true requests exclusion/inclusion. The today's-flow
Settings override remains effective. Omitted settlement uses the supplied
Settings evaluation date; omitted NPV date uses settlement. Explicit dates are
not business-day adjusted. Valid empty-leg NPV remains zero without requiring an
evaluation date; spread inversion follows the unchanged core solver behavior.

## Conventions and solvers

`itofin.time.Compounding` and Go `Compounding` share these explicit integers:

| Convention | Value | Behavior |
| --- | ---: | --- |
| `Simple` | 0 | Simple accumulation |
| `Compounded` | 1 | Periodic compounding |
| `Continuous` | 2 | Continuous compounding |
| `SimpleThenCompounded` | 3 | Simple at/below `1/f`, periodic above |
| `CompoundedThenSimple` | 4 | Periodic at/below `1/f`, simple above |

Existing Frequency facade ordinals stay Annual=0, Semiannual=1, Quarterly=2,
Monthly=3; these map to native frequencies 1, 2, 4 and 12. Conventions are supplied
by the caller, never inferred from coupon frequency. Admissible negative rates
work; nonpositive simple factors/periodic bases and nonfinite inputs error.

Solver defaults are accuracy `1e-10`, maximum evaluations `100`, yield guess
`0.05` and spread guess `0.0`. Go nil solver fields select those defaults. Explicit
zero accuracy/count rejects. An explicit zero guess remains zero: the unchanged
yield solver brackets with `guess/10`, so a nonzero root with zero guess can fail
typed rather than silently selecting the default. A genuine zero-yield target
can return zero. Solver failures do not poison later queries on the same bond.

## Live floating dependencies and cache purity

Analytical queries are independent of the engine's lazy state. Frozen and
already-calculated bonds use current coupon market inputs for these queries,
without changing cached policy flags, frozen state, calculated state or engine
NPV. External quote notifications can themselves invalidate calculated state;
queries preserve the state present immediately before the analytical call.

Guards inspect only surviving, non-ex-coupon flows. Clean outputs and Clean
inversion also inspect the actual accrued coupon group, including negative
ex-coupon accrual. Dirty routes do not require unrelated accrued-coupon data.
Retained Leg views keep their original coupon positions and see later pricer
replacement. Redemption views do not acquire a floating-coupon guard.

Actual floating fixings still need their index Settings evaluation date and
required history/pricer/volatility. All-zero gearing has fixed cashflows and
avoids those irrelevant requirements. The Rust at-par/indexed policy is read
live; native QuantLib pricers sample policy, so policy oracles use separately
constructed native cases rather than a global-toggle assumption.

## Verification and scope

Tracked QuantLib 1.43 fixtures cover all routes, both quote types, five
compounding modes, four frequencies, hybrid thresholds, negative rates, stubs,
ex-coupon accrual, two faces, inclusion policy and frozen live inputs. Independent
closed-form zero-coupon checks supplement native prices. Generated stubs, enum
integer/member accounting and unchanged static-typing diagnostics are required.

The yield and Z-spread routes above do not add ATM-rate helpers or an
InterestRate facade. Ordinary duration, convexity and BPV/YVBP are documented
below; callable-bond analytics remain separate. No core algorithm, version or
existing engine-based API is changed.


## Ordinary bond risk

These additive methods expose existing `BondFunctions` math for ordinary fixed,
zero and floating-rate bonds. They do not require a pricing engine, attach an
engine, change cached engine state or replace callable-bond effective risk.
Callable backing is rejected explicitly, even though it inherits `Bond` methods.

| Python `Bond` method | Go `*Bond` method | Meaning |
| --- | --- | --- |
| `duration` | `Duration` | Simple, Macaulay or Modified duration |
| `convexity` | `Convexity` | Flat-yield second derivative divided by value |
| `basis_point_value` | `BasisPointValue` | Signed currency value for a +1 bp yield change |
| `yield_value_basis_point` | `YieldValueBasisPoint` | Yield change for a +0.01 currency value change |

Python takes `(yield_rate, day_counter, compounding, frequency, *, settlement=None)`.
`duration` additionally takes keyword-only `duration_type=Duration.Modified`, with
`Duration` imported from `itofin.instruments`. Go takes `(rate, BondYieldConfig)`;
`Duration` takes `(rate, DurationType, BondYieldConfig)`, using `SimpleDuration`,
`MacaulayDuration` or `ModifiedDuration`. Macaulay requires **Compounded**, not a
hybrid or continuous convention. Each metric is queried independently, so an
unsupported Macaulay query does not prevent other valid risk queries.

The C scalar query is `itofin_bond_risk` with the existing yield config. Its
metric fields are 0 Simple, 1 Macaulay, 2 Modified, 3 convexity, 4 BPV, 5 YVBP.
Errors leave the output untouched. The usual thread/context/handle contract applies.

BPV and YVBP are **not per-100 quotes**. They use actual currency cash flows:
BPV scales with face amount and YVBP inversely with it. For a paying bond they
are normally negative. The native BPV convention retains the convexity correction
`0.5 * (convexity / 100) * value * 0.0001**2`; no rescaling or new math is added.
Duration and convexity are independent of face scaling.

Explicit settlement is passed unchanged; omission retains existing checked lag
and issue-date-floor behavior. Settlement-date and ex-coupon flows are excluded
according to native analytical conventions. Nontradable bonds fail. Floating
queries retain current fixing, forecast and coupon-pricer guards, even while an
engine cache is frozen. Live retained market inputs are read for analytical risk;
`freeze()` does not turn risk into a historical snapshot.

```python
from itofin.instruments import Duration

modified = bond.duration(
    0.04, DayCounter.actual365_fixed(), Compounding.Compounded,
    Frequency.Semiannual, duration_type=Duration.Modified, settlement=settlement,
)
bpv = bond.basis_point_value(
    0.04, DayCounter.actual365_fixed(), Compounding.Compounded,
    Frequency.Semiannual, settlement=settlement,
)
```

The shared 55-case test fixture was freshly generated from a QuantLib 1.43 wheel
on the existing analytics construction inputs. Source review uses QuantLib
`9863b578af0caa4cecabf697196533e84a8308b6` (1.43-dev), not a claim that this source
pin produced the wheel. Matching distribution notices retain upstream attribution.
Remaining BPS and ATM helpers are separate scope.
