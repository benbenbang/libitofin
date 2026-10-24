# Plain Ibor amortizing bonds

This additive factory uses the existing Ibor coupon leg, Bond principal inference
and discounting engine. The scalar `FloatingRateBond` constructor is unchanged.

| Boundary | Entry point |
| --- | --- |
| Rust | `FloatingRateBond::new_amortizing` |
| C | `itofin_amortizing_floating_rate_bond_new` |
| Go | `Session.NewAmortizingFloatingRateBond` |
| Python | `itofin.instruments.AmortizingFloatingRateBond` |

## Bounded contract

- Supply exactly one **positive, finite, nonincreasing** outstanding balance for
  each schedule period. Inputs are copied. A constant vector is permitted.
- Coupons have unit gearing. Empty spreads mean zero; shorter spread vectors
  repeat their last element. Spreads must be finite and cannot exceed the period count.
- Principal is repaid at par when balances decrease, on the preceding coupon's
  adjusted payment date. The final outstanding balance is repaid on the final
  adjusted payment date. Coupons precede principal on coincident dates.
- Contractual maturity remains the schedule end date, even when the last payment
  is moved to a later business day.
- The existing native Bond near-equality rule coalesces numerically close balance
  changes. Coupon balances still come from the supplied vector. This bounded
  factory preserves that native behavior rather than introducing different math.
- Payment convention, optional fixing lag/convention, issue date and ex-coupon
  terms use the existing calendar and supported-date guards. The schedule calendar
  is the payment calendar; there is no separate payment lag or redemption vector.
- Bond and index must share the **same settings object**, not merely equal dates.
- In this new factory, Rust/C/Go/Python settlement days, effective coupon fixing
  lag and retained index fixing lag are bounded by the actual supported Date
  maximum-minus-minimum span (1901-2199). Oversized lags fail before leg/calendar
  arithmetic. A particular date/calendar walk can still have a tighter valid range.
  This does not change day-bound behavior of the existing scalar constructor.

There are no caps, floors, in-arrears, overnight, callable, accreting or structured
payoff terms. Amortizing AssetSwap support is not part of this contract.

## Pricing and retained ownership

The result supports the common Bond NPV, settlement value, current notional,
clean/dirty price, accrued amount, cash-flow and redemption queries. Clean/dirty
prices and accrued amounts retain the existing per-100 current-notional units.
Yield and yield-to-price analytics preserve the existing
[BondFunctions contract](bond-analytics.md): zero outstanding settlement notional
is a typed nontradable error. This differs from native QuantLib `Bond::yield`
and `Bond::dirtyPrice`, which short-circuit to zero in that state. The native
fixture records retain those zero results; tests explicitly qualify this inherited
API-layer difference and assert nontradable errors, including unchanged C outputs.
Tradable yield comparisons retain the original numerical budgets. The scalar
bullet analytics obey the same unchanged rule.

Attach the existing `DiscountingBondEngine`; no separate coupon pricer is needed
for ordinary plain Ibor coupons.

Historical coupons require their actual past fixings. An unfixed fixing today
uses the existing settings policy; later historical observations supersede that
forecast. Construction itself does not require a fixing history or an evaluation
date. Future forecast-calendar checks remain deferred until the relevant amount
is requested.

The bond, its legs and individual cash-flow views retain the live index/settings/
curve graph. Forward and discount quote updates remain observable after their
original wrappers are released. Freeze/unfreeze and explicit recalculation retain
the existing Bond behavior.

Amortization interleaves principal with coupons. Each retained cash-flow view
therefore maps its **actual full-leg position** to its original coupon metadata.
Principal entries have no fixing/forecast guard. Releasing the bond or original
index wrapper does not change that map.

## Errors and language boundaries

- C preserves caller-owned outputs on failure, requires correctly typed handles
  from the same context, and confines context operations to the creating thread.
- Go returns the existing `*FloatingRateBond`, rejects mixed sessions and closed
  inputs, and runs native operations through its locked-thread session worker.
  Closing a session invalidates all its views.
- Python exposes a named `Bond` subclass with retained `index()` and
  `set_coupon_pricer()` methods. It is unsendable, as are the existing bond classes.
- Nonfinite amounts are errors, not successful NaN/Inf results. Invalid balances,
  spread vectors, settings identity, supported dates and calendar walks are rejected.

## Examples

- [Python example](https://github.com/benbenbang/libitofin/blob/main/example/python/amortizing_floating_bond.py)
- [Go example](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/amortizing_floating_bond/main.go)

See [bond bindings](bond-bindings.md) for common pricing/lifecycle contracts and
[Go ownership](https://github.com/benbenbang/libitofin/blob/main/docs/go-binding-contract.md) for context and session rules.
