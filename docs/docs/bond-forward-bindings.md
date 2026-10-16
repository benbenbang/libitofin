# Bond forward bindings

Python `BondForward` and Go `Session.NewBondForward(BondForwardConfig)` wrap the
existing native lazy forward. They retain the original fixed-rate or zero-coupon
plain bond, its actual engine, settings and checked calendar metadata. Releasing
an external bond/curve wrapper does not detach the forward. Floating-rate and
callable bonds are deliberately rejected. These additive bindings are not marked
released and do not implement a general futures or arbitrary cash-flow contract.

## Contract and units

- Supply bond, value date, delivery date, Long/Short position, nonnegative finite
  dirty delivery strike, settlement days, day counter, calendar, convention and
  financing discount curve. Income discounting defaults explicitly to that same
  financing curve; an independent live income curve is optional.
- The delivery date is adjusted using the supplied convention. Settlement is
  the later of value date and evaluation date advanced by business settlement
  days, using Following even when settlement days is zero.
- `fair_forward_price` / `FairForwardPrice` returns the native dirty delivery
  quote. `clean_forward_price` / `CleanForwardPrice` subtracts bond accrued
  interest at delivery. `npv` / `NPV` uses the dirty strike and reverses sign
  for Short. No engine is installed on the forward itself.
- **Native upstream units are intentionally preserved:** dirty spot is a per-100
  quote, but intervening income is raw currency. The subtraction is dimensionally
  consistent only for non-amortizing face-100 bonds. A face-250 bond retains the
  unusual upstream result, not an implicitly rescaled economic quote.
- Income includes payments strictly after forward settlement and on delivery,
  including redemption. There is no additional ex-coupon filtering of income.
  The underlying bond's own settlement and engine determine dirty spot.
- Curves, underlying engine replacement and evaluation-date updates stay live.
  Silent cash-flow settings are refreshed through the original bond facade.
  `freeze`, `unfreeze`, `recalculate` (Go capitalized) act on one real native
  forward cache; forced recalculation does not unfreeze it.
- Expired NPV is zero; fair and clean forward quotes report an error because no
  delivery quote remains. Metadata dates, strike, position and expiry are exposed.
- Go objects must share one live session. Python wrappers are thread-confined.
  Existing handle, pointer, retained ownership and source calendar limits apply.

## Python

```python
from itofin import Settings
from itofin.instruments import BondForward, Position, ZeroCouponBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter

settings = Settings()
settings.set_evaluation_date(Date(2, 1, 2025))
calendar = Calendar.null_calendar()
dc = DayCounter.actual365_fixed()
curve = FlatForward(Date(2, 1, 2025), 0.03, dc)
bond = ZeroCouponBond(2, calendar, 100.0, Date(2, 7, 2026), settings)
bond.set_engine(DiscountingBondEngine(curve, settings))
forward = BondForward(
    bond, Date(2, 1, 2025), Date(2, 7, 2025), Position.Long, 95.0,
    0, dc, calendar, BDC.Unadjusted, curve,
)
print(forward.fair_forward_price(), forward.clean_forward_price(), forward.npv())
```

Go's config uses `Bond: &zero.Bond` or `Bond: &fixed.Bond`, `PositionLong` or
`PositionShort`, and `IncomeDiscount: nil` for the native default. C uses
`ItofinBondForwardConfig` and checked value/integer/control entry points.
The pinned compiled QuantLib 1.43 fixtures exercise both plain product types,
Long/Short signs, omitted/distinct income curves, coupon/redemption delivery
boundaries and face-100/250 conventions. Retention, live updates, frozen caches,
invalid inputs and expired quotes have dedicated facade regressions.
