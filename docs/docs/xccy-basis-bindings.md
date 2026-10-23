# Plain cross-currency Ibor basis swaps

`XccyBasisSwap` is available in Python and Go/C as an additive facade over the
existing constant-notional Rust instrument. It pays the base leg and receives
the quote leg, including mandatory initial and final principal exchanges.
Positive base and quote notionals need not match at the supplied spot rate.

## Supported contract

- Plain Ibor indexes, schedules and day counters, with one quote-leg spread.
  Overnight/day-tenor indexes, resetting/MtM notionals and FX-linked cashflows
  are outside this constructor.
- Explicit base/source and quote/target currencies, matching index currencies.
  The copied immutable `spot` is quote units per base unit. It is not a live quote.
  Core currency equality uses names, not codes alone; use provided presets.
- Four potentially distinct curves: each index retains its forwarding handle,
  and the swap retains its two supplied discount handles. Both discount curves
  must share a reference date. Curves themselves have no currency metadata.
- Omitted payment convention means Following. Principal exchanges use each
  schedule's convention, at the common earliest accrual start/latest accrual end,
  not the last coupon payment date.
- Default settlement cutoff and NPV date follow the evaluation date in the exact
  shared Settings object. Optional fixed settlement/NPV dates and inclusion are
  supplied together. Today's cashflow override still takes precedence.
- Optional spot settlement interprets spot on that fixed date. Effective conversion
  is `spot * quote_discount(spot_date) / base_discount(spot_date)`. Spot settlement
  must not precede the common discount reference date.

## Values and units

| Python | Go | Meaning |
| --- | --- | --- |
| `base_leg_pv()` | `BaseLegPV()` | Contractual base-currency PV before the paid-leg sign |
| `quote_leg_pv()` | `QuoteLegPV()` | Contractual quote-currency PV before the received-leg sign |
| `quote_leg_bps()` | `QuoteLegBPS()` | Quote-currency sensitivity for one basis point of spread, excluding principal |
| `npv()` | `NPV()` | Base-currency signed NPV: quote PV / effective spot - base PV |
| `npv_in_quote()` | `NPVInQuote()` | Quote-currency signed NPV: quote PV - effective spot * base PV |
| `fair_quote_basis_spread()` | `FairQuoteBasisSpread()` | Absolute quote spread zeroing NPV, not a spread shift |

Each query rereads current curve quotes, fixings and Settings; there is no lazy
instrument cache or freeze API. Leg-only queries do not demand the other leg's
fixings. Quote BPS still needs quote coupon amounts because this existing core
instrument uses combined NPV/BPS measurement. An expired leg contributes zero;
expired fair spread fails because spread sensitivity is zero.

## Ownership and errors

Python objects are unsendable. Go Session serializes native calls and rejects
foreign-session, closed and nil inputs. Original index, schedule, calendar, curve
and Settings graphs remain retained after wrapper release. Currency accessors
return independent copies; close returned Go Currency handles. C outputs change
only on success; all pointers follow the existing C caller contract.

Invalid/nonfinite notionals, spread or spot, mismatched currencies/Settings,
missing fixings and invalid curve/date domains produce errors. No engine attachment,
live spot setter, public curve relink setter or general cashflow-leg facade is added.
Rebuild the swap when replacing spot or contractual terms.

## Examples and oracle provenance

- Python: `example/python/xccy_basis.py`.
- Go: `sdk/go/examples/xccy_basis/main.go`.
- Focused fixtures reuse the tracked whole-class native oracle from
  `crates/libitofin/tests/fixtures/medium3_xccy_basis/`: actual compiled QuantLib
  1.43 `ConstNotionalCrossCurrencyBasisSwap` and discounting engines in both currencies.
- Source audit revision: `9863b578af0caa4cecabf697196533e84a8308b6`, separate from
  the compiled package revision. Native absolute tolerances remain2e-9 for values
  and2e-13 for fair spread. Original fixture provenance and limitations remain intact.

This is bounded Ibor-product availability, not full QuantLib cross-currency parity.
