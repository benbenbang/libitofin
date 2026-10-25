# Bond and leg payment inspection

C, Go and Python expose payment navigation over existing retained cash-flow graphs.
These APIs are additive and do not install an engine or calculate a bond's NPV.

| Query | Python | Go |
| --- | --- | --- |
| Accrual span | `leg.start_date()`, `leg.maturity_date()` | `leg.StartDate()`, `leg.MaturityDate()` |
| Bond accrual start | `bond.start_date()` | `bond.StartDate()` |
| Bond tradability | `bond.is_tradable(settlement=None)` | `bond.IsTradable(settlement)` |
| Previous/next payment date | `previous_cash_flow_date`, `next_cash_flow_date` | `PreviousCashFlowDate`, `NextCashFlowDate` |
| Previous/next actual amount | `previous_cash_flow_amount`, `next_cash_flow_amount` | `PreviousCashFlowAmount`, `NextCashFlowAmount` |

## Dates and settlement policy

Leg navigation takes the evaluation `Settings`, an optional settlement date, and
an optional `include_settlement_date_flows` policy. Go passes these in
`CashFlowsPaymentConfig`. An omitted settlement date uses the evaluation date,
which must be set. An omitted inclusion flag uses the Settings reference-date
policy.

Bond navigation accepts an optional settlement date. An omitted date uses the
bond's calendar-aware settlement date, floored at its issue date. Bond helpers
pass settlement-date exclusion to the core. The native Settings
`include_todays_cash_flows` override still takes precedence when settlement is
the evaluation date: setting it to true can make today's payment the next one,
even when the explicit inclusion flag was false. This behavior is shared by both
Bond and Leg helpers.

Previous and next refer to stored payment order. The supported bond and leg
builders keep their cash flows ordered. The leg start is the earliest coupon
accrual start or non-coupon payment date; maturity is the latest coupon accrual
end or non-coupon payment date. Neither is an adjusted-payment-only bound.
Existing `bond.maturity_date()` / `MaturityDate()` remains unchanged.

## Amounts and missing payments

- Amounts are **actual currency payments**, not per-100 bond quotes.
- All consecutive flows sharing the selected payment date are added together.
  A final coupon and redemption therefore return one aggregate amount.
- Navigation is not NPV filtering. In particular, a coupon in its ex-coupon
  window remains part of its payment-date amount.
- No matching payment returns Python `None` or a nil Go pointer, distinct from
  an actual payment whose amount is zero. QuantLib's native amount helper uses
  zero when no date exists; this facade deliberately makes absence explicit.
- Date-only, span and tradability queries do not resolve amounts or fixings.
  Amount queries resolve only the selected date's flows and enforce their
  existing fixing/calendar/pricer guards, even if the parent bond is frozen.
  Non-finite aggregate amounts are errors, including finite-flow sum overflow.
- Tradability means outstanding notional is nonzero at settlement, not merely
  that the instrument's expiry predicate is false.

C uses `itofin_leg_span_date`, `itofin_leg_payment`,
`itofin_bond_inspection` and `itofin_bond_payment`. The payment result has
`present`, `date` and `amount` fields. No payment produces all zero fields;
a date-only query sets amount to zero. Failed calls leave the output unchanged.
Selectors and boolean fields accept only their documented values.

## Lifetime and provenance

Legs extracted from bonds retain their cash flows after the original wrapper is
released. CPI legs require the identical Settings object used by their index,
not a different Settings object with the same date. Go additionally requires the
same Session for every input. Releasing a required Settings handle makes a Go/C
Leg query fail; Bond's retained Settings continue to support Bond queries.

The 60-row native fixture in `sdk/go/testdata/payment_inspection/` covers
settlement boundaries, today's override, ex-coupon windows, actual principal
aggregation and missing payments. It was produced by
`scripts/generate_payment_inspection_oracle.py` using compiled QuantLib 1.43.
The recorded pinned source revision is a separate reference, not evidence that
it built that wheel. These cases do not establish complete BondFunctions parity.

Examples: [`example/python/payment_inspection.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/payment_inspection.py)
and [`sdk/go/examples/payment_inspection/main.go`](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/payment_inspection/main.go).
