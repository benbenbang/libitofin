# Callable bond bindings

Go and Python construct the existing hard-call/put callable fixed-rate and
zero-coupon products. These additive bindings are not marked released.
Both retain the actual callable instrument and its single lazy `Bond` base.
There is no separate cash-flow bond or second valuation cache.

## Python

```python
from itofin import Settings
from itofin.instruments import Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond
from itofin.models import HullWhite
from itofin.pricingengines import TreeCallableFixedRateBondEngine
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule

settings = Settings()
settings.set_evaluation_date(Date(15, 3, 2026))
calendar = Calendar.null_calendar()
day_counter = DayCounter.actual360()
schedule = Schedule(
    Date(15, 1, 2026), Date(15, 1, 2030), Frequency.Annual,
    calendar, BusinessDayConvention.Unadjusted,
)
rights = [Callability(100.0, CallabilityType.Call, Date(15, 1, 2028), BondPriceType.Clean)]
fixed = CallableFixedRateBond(2, 250.0, schedule, [0.045], day_counter, rights, settings)
zero = CallableZeroCouponBond(2, calendar, 250.0, Date(15, 1, 2030), day_counter, rights, settings)
curve = FlatForward(Date(15, 3, 2026), 0.031, day_counter)
model = HullWhite(curve, 0.1, 0.01)
engine = TreeCallableFixedRateBondEngine(model, 100, settings)
fixed.set_tree_engine(engine)
zero.set_tree_engine(engine)
print(fixed.npv(), fixed.clean_price(), zero.settlement_value())
```

- `Callability` is immutable. `price()`, `price_type()`, `call_type()` and `date()`
  return its checked native values. Quote type defaults to Clean; price is per 100 of face.
- `callability()` returns a copied schedule snapshot. Rights retain their supplied
  order, including past, coincident, mixed call/put and unsorted events. Empty
  schedules are valid. No constructor sorting or deduplication occurs.
- Both callable classes inherit the existing nonconstructible `Bond` interface,
  not the concrete straight-bond classes. Fixed adds `frequency()` and
  `day_counter()`; zero adds its explicitly supplied payment `day_counter()`.
  Construction retains native frequency. A supported product with Once or
  NoFrequency still constructs; the restricted common frequency getter reports
  an unsupported-frequency error only when called.
- Python objects and their native dependencies remain thread-confined.

## Go and C

Go constructs validated immutable-value records with
`NewCallability(price, priceType, callType, date)`. `CallabilitySchedule` is a
slice of records whose fields are `Price`, `PriceType`, `Type` and `Date`.
Direction constants are `CallabilityCall` and `CallabilityPut`, avoiding the
existing option `Call`/`Put` names.

- `Session.NewCallableFixedRateBond(CallableFixedRateBondConfig)` and
  `Session.NewCallableZeroCouponBond(CallableZeroCouponBondConfig)` retain the
  actual native callable owners. Both embed the existing `Bond` interface.
- The fixed config embeds `FixedRateBondConfig` and adds `Callability`.
  The zero config embeds `ZeroCouponBondConfig` and adds `DayCounter` and
  `Callability`. Dependencies must belong to the same live session.
- `Callability()` returns a copied value slice; fixed exposes `Frequency()` and
  `DayCounter()`, while zero exposes `DayCounter()`.
- C uses the by-value `ItofinCallability` record, checked by
  `itofin_callability_new`. Callable constructors revalidate every supplied
  record, including records created directly by C/Go callers.
- `itofin_callable_bond_callability` uses a count and caller-owned output buffer.
  Normal registry, pointer, handle, session and thread contracts apply.

## Supported constructor controls

| Control | Callable fixed | Callable zero |
| --- | --- | --- |
| Calendar | Retained schedule calendar | Explicit calendar |
| Day counter | Accrual day counter | Required payment day counter |
| Payment convention | Following default | Following default |
| Redemption per 100 | 100 default, zero supported | 100 default, zero supported |
| Issue date | Optional | Optional |
| Ex-coupon controls | Period, calendar, convention, end-of-month | Not applicable |
| Payment-calendar override | Not supported | Main calendar is explicit |
| First-period day-counter override | Not supported | Not applicable |

Python omits unsupported fixed parameters. Go/C reject explicitly supplied fixed
payment-calendar or first-period-day-counter overrides rather than ignoring them.
Face is positive and finite. Redemption and exercise prices are finite and
nonnegative; generated/scaled cash-flow amounts must stay finite. Existing
straight-bond positive-redemption policy is unchanged.

Fixed ex-coupon convention defaults to Unadjusted and end-of-month to false;
an omitted ex-coupon calendar is NullCalendar, including when a period is supplied.
Negative ex-coupon periods are rejected. Explicit zero-length ex-coupon
period retains native payment-date semantics. Clean exercise prices include
indenture accrued interest independently of market ex-coupon accrued interest.

Exercise dates must be nonnull and no later than contractual maturity. Zero also
rejects rights after adjusted redemption. Checked calendar metadata and bounded
settlement/payment/ex-coupon traversals reject unsupported holiday-table dates
and date overflow. Unadjusted mathematical paths do not require unused holiday
lookups. Issue dates floor settlement and remain subject to native validation.

## Inherited analytics and lifecycle

The inherited [yield/curve/Z-spread methods](bond-analytics.md) value remaining
**contractual straight cash flows, without exercise optionality**. They are not
option-adjusted callable quotes or OAS. They read the same immutable bond and do
not install an engine, refresh flags, unfreeze, calculate or change the cache.

`npv`, settlement value, clean/dirty quotes, expiry, freeze/unfreeze and recalculate
dispatch through the actual callable native instrument. An unexpired bond without
a pricing engine reports an error. Native expired instruments return zero, even
when frozen. Freeze before an initial calculation does not fabricate results.
Callable zero silently refreshes inclusion-policy flags through its actual native
base, preserving freeze. Existing plain/floating and fixed policy behavior remains
unchanged.

The generic `DiscountingBondEngine` is the wrong engine contract for either
callable product. `Bond.set_engine` / `Bond.SetEngine` reject it before mutating
attachment or cache state. Use the additive `set_tree_engine` / `SetTreeEngine`
methods for the typed callable tree engine, or `set_black_engine` / `SetBlackEngine`
for the typed single-European Black engine. Existing base signatures are unchanged.

## Hull-White tree engine

`TreeCallableFixedRateBondEngine(model, time_steps, settings)` retains the existing
`HullWhite` model and its actual live curve graph, with positive required time
steps and explicit retained settings. Go uses
`Session.NewTreeCallableFixedRateBondEngine(model, timeSteps, settings)` and both
concrete callable products expose `SetTreeEngine`.

- Bond and engine must share the **same Settings allocation**, not merely equal
  evaluation dates. A mismatch fails before replacing the real installed engine.
  HullWhite has no settings member, so there is no invented model/settings check.
- Attachment and all valuation methods use the actual callable's one lazy base.
  Replacing a tree engine detaches the former engine observer registration.
- Engine/model/curve/settings wrappers can be released after attachment: native
  dependencies remain retained. Live curve quotes and model notifications
  invalidate the attached bond. Existing freeze, unfreeze, recalculate and native
  expiry behavior remains intact.
- The lattice rebuilds with mandatory exercise/payment times on each calculation.
  Empty, multiple, mixed, unsorted and coincident hard rights retain native order.
- NPV is currency at the **model curve reference date**. Settlement value is NPV
  divided by the model curve's unspread settlement discount. Clean/dirty quotes
  are per 100 of current notional, using the actual typed native methods.
- Constructor does not require an evaluation date or price eagerly. Actual use
  propagates unset evaluation, settlement-before-reference, invalid discounts,
  unsupported calendar dates and nonfinite pricing errors without clamping or
  enabling extrapolation. Zero redemption with a future positive put can have
  positive value.
- The inherited stateless yield/curve/Z-spread methods remain contractual
  straight-cashflow analytics, not option-adjusted prices. They do not modify the
  installed tree, lazy cache or freeze state on success or error.

This is the existing Hull-White tree engine only. No generic/G2 model, alternate
model-fitting curve, public continuous-spread pricing shortcut or new model
calibration/mutation API is added.

## Single-European Black engine and pure IV

`BlackCallableFixedRateBondEngine(yield_volatility, discount_curve, settings)`
retains the actual live `SimpleQuote`, `YieldTermStructure` and Settings graph.
Both concrete products add `set_black_engine(engine)` without widening the base
`set_engine` or changing `set_tree_engine`. Exact Settings allocation equality
is checked before attachment. Constructor and attachment do not price eagerly.

```python
from itofin.pricingengines import BlackCallableFixedRateBondEngine
from itofin.quotes import SimpleQuote

relative_yield_volatility = SimpleQuote(0.30)
black_engine = BlackCallableFixedRateBondEngine(relative_yield_volatility, curve, settings)
fixed.set_black_engine(black_engine)
target = fixed.clean_price()
print(fixed.implied_volatility(target, curve))
```

- Volatility is **relative forward compounded-yield volatility**, not absolute
  yield volatility, Hull-White sigma, price volatility or an option-vol surface.
  Modified duration converts it to forward price volatility. The volatility clock
  is Actual/365 Fixed from evaluation, not the discount curve reference date.
- Actual Black use requires **one original and one still-active hard right**.
  Two supplied rights with one remaining still reject. Empty, past,
  settlement-day or redemption-date rights reject while the bond is unexpired.
  Whole-instrument expired quotes may return native zero before these checks;
  IV always rejects expiry. This is not a multiple-exercise approximation.
- Once/NoFrequency payment conventions use Annual compounded forward yield
  internally without adding common facade enum constants.
- Clean strikes include indenture accrued, including full ex-coupon coupon
  entitlement where required. Settlement-date coupon ownership and exercise-date
  yield ownership are distinct. Corrected forward units include the settlement
  discount factor absent from raw QuantLib 1.43 positive-lag Black prices.
- Exact zero forward yield produces intrinsic option pricing. Tiny nonzero yields
  are not epsilon-clamped: negative forward yield with positive relative vol
  rejects, while zero vol retains native intrinsic/signed-zero behavior.
- Live volatility/curve quotes and evaluation changes invalidate the same cache.
  Freeze, unfreeze, recalculate and expiry retain native behavior. Black-to-Black,
  tree-to-Black and Black-to-tree replacement detach former input notifications.
  Releasing Python wrappers does not release dependencies still held natively.

Both callable products expose:

```python
bond.implied_volatility(
    target_price, discount_curve, price_type=BondPriceType.Clean,
    accuracy=1e-10, max_evaluations=100, min_vol=0.0, max_vol=1.0,
)
```

| IV input | Contract |
| --- | --- |
| Target | Clean default or Dirty, per 100 settlement quote |
| Explicit curve | May differ from installed engine's curve |
| Accuracy | Finite and positive |
| Maximum evaluations | Positive, strict actual Black pricing-probe budget |
| Bracket | Finite `0 <= min_vol < max_vol` |
| Lower/upper endpoint | Lower may need one probe; upper generally needs two |

A negative clean target is valid when adding settlement accrued gives finite
nonnegative dirty price. Dirty negative, nonfinite targets, unsupported rights,
expiry, failed curves, exhausted budgets or targets outside the bracket report
errors, never a clamped root. A near-intrinsic or exact-zero-yield price may not
uniquely identify the original quoted volatility: the returned root is a bracketed
repricing solution, not a promise to recover an unidentifiable input. There is no
guess or settlement override input.

The original Black price matrix's six zero-coupon Call/Put cases with strike100,
volatility0.1 and settlement lags0/3/15 are specifically near-intrinsic at floating
price precision. Their independent prices are retained; IV is qualified by
bracket, repricing and unchanged-state contracts rather than an unknowable
original-volatility assertion. Separate strike85 counterparts and all16 historical
IV cases retain the strict `2e-8` known-volatility tolerance. This qualification
is not a blanket relaxation of IV checks or pricing tolerances.

IV reads the actual immutable callable and performs only checked default
settlement preflight. It does not require, attach, prepare or refresh an installed
engine, consume pending policy flags, calculate/unfreeze the bond or mutate
cached prices, engine arguments/results, original quote or observer registrations.
The same purity holds with no engine, cached/frozen tree or Black, on success
and failure. Stateless inherited straight-cashflow analytics remain unchanged.

Go uses `Session.NewBlackCallableFixedRateBondEngine(yieldVolatility,
discountCurve, settings)`, concrete `SetBlackEngine` and
`ImpliedVolatility(targetPrice, discountCurve, options)`.
`CallableImpliedVolatilityOptions` has `PriceType` and optional pointer controls
`Accuracy`, `MaxEvaluations`, `MinVol`, `MaxVol` with the same defaults. C exposes
`itofin_black_callable_bond_engine_new`,
`itofin_callable_bond_set_black_engine` and
`itofin_callable_bond_implied_volatility`; optional control presence bits are
1, 2, 4 and 8 respectively. Existing session/thread/output contracts apply.

## Scope and evidence

Constructor, inherited analytics, ownership, calendar/date boundaries, typed-engine
failure and lazy-dispatch regressions cover the bindings. Independent QuantLib 1.43
fixed/ex-coupon and nonflat zero fixtures exercise tree NPV/settlement/clean/dirty
quotes with unchanged native/core tolerances. Mixed expired/future direction
fixtures retain the known native original-schedule bug separately and use fresh
settlement-eligible native rights as the aligned pricing oracle. Existing corrected
Rust silent-zero flag semantics are preserved. No core algorithm or public Rust
API changes are made.

Soft calls, convertibles, generic/G2 trees, full callable volatility surfaces and
multi-exercise Black approximations remain outside the approved binding scope.

## Explicit-tree OAS

Both callable concrete classes provide `clean_price_oas` and `oas` in Python,
`CleanPriceOAS` and `OAS` in Go. Each takes an explicit retained
`TreeCallableFixedRateBondEngine`. These immutable analytics do not attach that
engine, populate the bond cache, thaw a frozen result, refresh silent Settings
flags or alter the model, curve or installed pricing engine. The explicit tree
and bond must share the exact Settings allocation, not merely equal dates.

- Spread input and recovered OAS are decimal conventional rates, not basis points.
- Select day counter, compounding and frequency explicitly. Conversion uses the
  curve zero rate at contractual maturity, not the adjusted redemption date.
- Targets are clean settlement quotes per 100. Settlement normalization uses the
  unspread curve discount and actual notional. This differs from QuantLib's
  unnormalized positive-settlement-lag helper quote.
- Omitted settlement uses checked native settlement. A supplied settlement is
  an actual date, without calendar adjustment or settlement lag, and may be a
  nonbusiness day or precede evaluation where native date bounds permit it.
- Solver accuracy and guess are **continuous additive short-rate shift units**.
  Defaults are accuracy `1e-10`, maximum evaluations `100`, guess `0.0`.
  Explicit zero accuracy or budget rejects; zero spread or guess is valid.
  Budgets count actual native lattice probes and are strict.

```python
price = bond.clean_price_oas(
    0.015, tree, day_counter, Compounding.Compounded, Frequency.Semiannual,
)
spread = bond.oas(
    price, tree, day_counter, Compounding.Compounded, Frequency.Semiannual,
    accuracy=1e-10, max_evaluations=100, guess=0.0,
)
```

Go uses `CallableOASConfig` and `CallableOASSolveConfig`, with existing optional
`AnalyticsSolverConfig` controls. A nil settlement means omission; an explicit
zero `Date` rejects rather than falling back to default settlement. The C config
has settlement-presence bit `1`, with unknown bits rejected; errors leave the
result pointer untouched. See `sdk/go/examples/callable-oas`.
