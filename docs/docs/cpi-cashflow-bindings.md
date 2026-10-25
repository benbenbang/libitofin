# Plain CPI cash flows

Rust, C, Go and Python support an explicit-base CPI coupon, plain swaplet
pricer and CPI leg. These are additive APIs. Existing zero-coupon inflation
swaps and year-on-year inflation instruments are unchanged.

## Supported contract

- The coupon rate is `fixed_rate * lagged_CPI(accrual_end) / base_CPI`.
- Coupon amount is rate times nominal times its accrual year fraction.
- Accrued amount uses CPI observed at the requested settlement date, **not**
  the coupon end. Ex-coupon accrual can be negative.
- Flat and Linear CPI observation interpolation are supported. Base CPI is
  supplied explicitly and must be finite with absolute value greater than
  `1e-16`. Its interpolation convention is the caller's responsibility.
- The plain pricer applies no convexity adjustment. Its discounted swaplet
  price is per unit nominal and requires a nominal discount curve. Rate and
  accrued-rate queries do not need that curve.
- Every CPI leg has a terminal indexed principal payment. `growth_only=true`
  subtracts the uninflated principal; `false` pays the full indexed principal.
  The observation follows the unadjusted contract end, not its adjusted payment.
- A zero fixed rate creates a fixed zero coupon that needs no inflation fixing.
  The terminal principal **still** needs its own CPI fixing or forecast curve.
- Nonnegative Days, Weeks, Months and Years observation lags are accepted.
  Invalid dates, missing fixings, missing curves and non-finite results fail.

## Language entry points

| Layer | Entry points |
| --- | --- |
| Rust | `CpiCoupon`, `CpiCouponPricer`, `CpiCashFlow`, `CpiLeg` |
| C | `itofin_cpi_coupon_*`, `itofin_cpi_pricer_value`, `itofin_cpi_leg_*` |
| Go | `Session.NewCPICoupon`, `NewCPICouponPricer`, `NewCPILeg` |
| Python | `itofin.cashflows.CPICoupon`, `CPICouponPricer`, `CPILeg` |

Attach a plain pricer before querying a directly constructed coupon's rate or
amount. Leg construction attaches its default plain pricer automatically.
The resulting generic `Leg` supports existing cash-flow inspection and NPV
functions. Use the **same Settings object** as the CPI index when pricing that
leg, including spread analytics. Separate objects with equal dates are not the
same settings. Go also requires every object to belong to the same Session.

C and Python leg facades materialize their payment and ex-coupon dates at
construction. `Build()`/`build()` clones this fixed-date cash-flow graph;
subsequent calendar holiday edits do not rebuild its dates. CPI histories,
linked inflation curves and discount curves remain live and retained after
input wrappers are released. Reconstruct the builder to adopt changed dates.
The Rust builder materializes dates on each `build()` call.

## Boundaries

Base-date-derived constructors, capped/floored individual CPI coupons, CPI
optionlet volatility models and convexity adjustments are not included here.
The cumulative single-payoff CPICapFloor instrument is a separate pricing path,
not a clamp on these coupons. Vectors carry their last value forward; leg
notionals have at most one entry per coupon plus terminal payment, and rates
at most one entry per coupon. Irregular schedule reference periods and optional
ex-coupon periods follow the schedule/payment calendars supplied.

Independent fixtures in `sdk/go/testdata/cpi-oracle-*.json` are produced by
`scripts/generate_cpi_oracle.py` using QuantLib 1.43. The recorded pinned C++
source is a separate reference, not a claim that it built the Python wheel.
Fixture agreement covers the listed scenarios, not full QuantLib parity.

Runnable examples: [`example/python/cpi_cashflows.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/cpi_cashflows.py)
and [`sdk/go/examples/cpi_cashflows/main.go`](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/cpi_cashflows/main.go).
Both use published historical CPI and a nominal discount curve, so no inflation
forecast curve is needed for their short maturity.
