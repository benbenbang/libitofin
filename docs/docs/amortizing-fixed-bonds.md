# Amortizing fixed-rate bonds

`AmortizingFixedRateBond` (Python) and
`Session.NewAmortizingFixedRateBond` (Go) build a plain fixed-rate bond with
scheduled reductions of principal. Both use the existing fixed coupon leg,
Bond principal construction and discounting engine. No new pricing formula is
introduced.

## Contract

- Supply exactly one balance for each accrual period, `len(schedule.dates()) - 1`.
- Balances must be finite, strictly positive and nonincreasing. Equal adjacent
  balances are allowed. No final zero entry is supplied.
- Principal reductions are paid at par on the **preceding coupon payment date**.
  The last balance is redeemed at par on the final coupon payment date.
- Maturity is the schedule's final accrual date. A holiday adjustment can move
  the final cash payment after this date. Current notional follows actual
  principal payment dates, not the unadjusted accrual dates.
- Existing Bond notional coalescing remains in force: numerically near-equal
  adjacent balances can be treated as unchanged for principal construction,
  although the coupon leg retains the supplied balances. This is native
  QuantLib behavior, not an exact-principal arithmetic promise.
- Coincident coupon and principal payments remain separate cashflows, coupon
  first. Read `redemptions()` / `Redemptions()` to inspect all principal payments.
- Clean/dirty quotes and accrual are per 100 of **current** outstanding notional.
  NPV and settlement value are currency amounts. Existing yield analytics use
  the same current-notional convention after amortization.
- Payment calendar/convention, issue-date settlement floor, optional first-period
  day counter and ex-coupon options match the fixed-bond facade. Ex-coupon
  detaches coupon entitlement, not principal entitlement. Omit the ex-coupon
  period for no ex-coupon convention. As with the existing Option-based leg,
  an explicit zero-length period is distinct from omission in this port; native
  QuantLib's zero-period sentinel does not make that distinction.
- Settings and attached discounting engine inputs remain live and retained.
  Existing freeze/unfreeze, settlement/date/event flags and expiry apply.

The Rust additive factory is `FixedRateBond::new_amortizing`; the Go result is
`*FixedRateBond`, retaining fixed metadata and common `Bond` methods. Python
has a separately named `AmortizingFixedRateBond` class extending `Bond`.
This is a bounded facade, not exact native class parity. Existing scalar
`FixedRateBond` constructors retain their single-redemption restriction.

## C ABI

`itofin_amortizing_fixed_rate_bond_new` takes a typed
`ItofinAmortizingFixedBondConfig`, separate notional/coupon arrays and their
lengths. Its config deliberately has no scalar face amount or redemption quote.
It follows the existing context/thread ownership, typed-handle and output-on-error
contracts. Go sessions serialize native calls and reject foreign/closed handles.

## Scope

Supported: plain, unlevered fixed coupons and par principal with the existing
coupon rate-vector rules and discounting/yield interfaces.

Not supported by this factory: accreting or zero/negative balances, arbitrary
principal schedules, custom redemption quotes, payment lags, callable/structured
bonds or amortizing asset swaps. An AssetSwap still requires a bullet bond with
one redemption. A constant-balance vector is economically a bullet bond.

Existing BondForward composition is unchanged. Its native dirty-quote-minus-raw-
income units remain dimensionally consistent only for a non-amortizing face-100
bond. Amortizing/non-100 forwards retain that documented unusual native result;
this factory does not repair or newly promise economic forward-quote units.

## Examples and oracle

- Python: [`example/python/amortizing_fixed_bond.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/amortizing_fixed_bond.py)
- Go: [`sdk/go/examples/amortizing_fixed_bond`](https://github.com/benbenbang/libitofin/tree/main/sdk/go/examples/amortizing_fixed_bond)

Native fixtures use independently selected cases: declining/plateau/constant
balances, adjusted weekend payments, pre/post-principal dates, ex-coupons,
settlement-day variants, live curve changes and post-maturity states. A single
JSONL fixture is reused by C-ABI, Python and Go tests. Regeneration uses the
QuantLib 1.43 wheel in an isolated oracle environment. The separately inspected
source is revision `9863b578af0caa4cecabf697196533e84a8308b6`; that revision is
not claimed as the wheel's build identity.
