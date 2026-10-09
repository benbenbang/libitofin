# Bond forwards

`instruments::BondForward` is an additive Rust-core product. It retains a
`SharedMut<Bond>` and prices without its own engine. The underlying bond must
have an appropriate pricing engine. There are no new C, Go or Python bindings.

## Construction and live inputs

- `new` uses the financing handle for both financing and income discounting.
  This is an explicit convenience, not a QuantLib empty-curve fallback.
- `with_income_curve` accepts a separate income curve.
- Both accept value date, delivery date, position, dirty strike, settlement days,
  day counter, calendar and delivery adjustment convention.
- Settings come from the retained bond. There is no competing evaluation-date
  source. Quote changes, curve relinks, bond-engine replacement and evaluation
  date changes invalidate previously computed results.
- Reference-date/today-flow flags do not notify observers, matching the existing
  Settings contract. After changing them, explicitly `recalculate()` the bond
  and forward if either has already been priced.
- Day counter is retained metadata, not an extra compounding convention.

## Dates and quotes

| Quantity | Convention |
| --- | --- |
| Forward settlement | Maximum of value date and evaluation date advanced by settlement business days; zero days rolls Following |
| Bond spot | Engine-priced `Bond::dirty_price()` at the bond's own settlement |
| Delivery | Adjusted using the forward calendar and convention |
| Income | All bond flows after forward settlement and on or before delivery |
| Dirty fair quote | `(dirty spot quote - discounted income) / financing discount(delivery)` |
| Clean fair quote | Dirty fair quote minus bond accrued quote at adjusted delivery |
| Long NPV | `(dirty fair quote - dirty strike) * financing discount(delivery)` |
| Short NPV | Negative long NPV |

Curve discounts are used at their native reference date, not rebased to forward
settlement. Income includes coupons, amortization and redemption. A payment on
settlement is normally excluded; a payment on delivery is included. The actual
cash-flow `has_occurred` rule is retained: the today-flow flag can override the
explicit exclusion when settlement equals evaluation date.

## Important upstream conventions

QuantLib subtracts **raw currency cash-flow income** from a **spot quote per
100**, without rescaling. This is dimensionally consistent for a non-amortizing
face-100 bond only. Non-100 and amortizing inputs preserve the exact upstream
result, including its mixed-unit behavior. This API does not silently repair it
or reinterpret strike as a currency notional.

The upstream income loop does not exclude trading-ex-coupon payments. Spot
pricing and accrued interest still obey the bond's own ex-coupon conventions.
A clean strike is not accepted automatically: convert it to a dirty quote first.

## Checked errors and expiry

Construction rejects null dates, nonfinite or negative strikes, delivery before
value date after adjustment and unrepresentable business-day adjustments.
Pricing rejects missing evaluation dates, borrowed underlying bonds, missing
engines/curve links, out-of-range curve queries, nonpositive/nonfinite discounts,
and nonfinite cash flows or results. Curves are not forcibly extrapolated.

NPV is zero once adjusted delivery has occurred at forward settlement, subject
to the reference-date-event flag. Fair/clean price requests after expiry return
an error instead of exposing QuantLib's uninitialized or stale price members.
Inherited pricing engines, calendars and day counters retain their own limits;
this product does not add exception-catching around arbitrary user callbacks.

## Independent validation

The tracked generator under `tests/fixtures/medium_bond_forward` compares against
compiled QuantLib **1.43**, separately identifying the inspected source revision
`9863b578af0caa4cecabf697196533e84a8308b6`. The Rust tests bake the generated
values and need no Python runtime. Cases distinguish all three curves, delivery
coupon/redemption, value-date floors, positions, non-100/amortizing notionals,
ex-coupon/today rules and live quote/relink/settings updates.
