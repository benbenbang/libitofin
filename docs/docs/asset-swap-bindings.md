# Asset swap bindings

C, Go and Python expose the existing Rust **par, full-maturity AssetSwap**.
The API is additive. It does not implement market swaps, amortizing or callable
bonds, overnight indexes, custom final repayments or a complete QuantLib facade.
Only existing plain fixed-rate and zero-coupon bullet bonds are accepted.

## Terms and results

| Item | Contract |
| --- | --- |
| `pay_bond_coupon` | Pay the bond leg and receive floating when true; reverse both signs when false |
| Clean price | Quote per 100 at the floating schedule start, not currency NPV |
| Spread | Absolute floating spread in decimal rate units |
| Floating schedule | Optional; defaults to backwards generation from bond settlement to full maturity using index tenor/calendar/convention, without EOM |
| Floating day counter | Optional; defaults to the index day counter |
| Engine | Live discount curve, identical bond/index/engine Settings, native date and flow defaults |
| NPV and leg NPVs | Signed currency amounts at the engine NPV date |
| Floating BPS | Signed currency change for a one-basis-point spread increase |
| Fair spread | Absolute spread zeroing NPV, not an incremental spread correction |

An explicit floating schedule must end at convention-adjusted full bond maturity.
Floating payments are Following-adjusted. The floating leg includes a dirty-price
upfront payment and final notional exchange. Nonpar bullet redemptions are retained.

**Fair clean price caveat:** the native formula zeroes NPV only when the upfront
payment is included in valuation. With upfront excluded on the reference date it
still returns the native formula, not a zero-NPV reconstruction. A seasoned deal
whose upfront predates the curve reference date fails rather than fabricating a
fair clean price.

## Python

```python
from itofin.instruments import AssetSwap

swap = AssetSwap(True, bond, 97.25, index, 0.0017, floating_schedule)
swap.set_engine(discount_curve, settings)
value = swap.npv()
fair_spread = swap.fair_spread()
```

Inspectors include clean price, spread, upfront date, payer direction and the
always-true par flag. Signed bond/floating NPVs, floating BPS, fair clean price,
calculate/recalculate, freeze/unfreeze, cached state and expiry are available.
Python classes are unsendable and retain their native inputs after wrapper deletion.

## Go and C

Go uses `Session.NewAssetSwap(AssetSwapConfig{...})` and
`AssetSwap.SetEngine(discount, settings)`. Pass the existing bond's `*Bond`, for
example `&fixedBond.Bond`. A nil `FloatingSchedule` generates the native default;
a nil `FloatingDayCounter` selects the index counter.

C uses `ItofinAssetSwapConfig` and `itofin_asset_swap_new`. Zero schedule/counter
handles select their respective defaults. `itofin_asset_swap_value` selects the
scalar query documented in the generated header. Outputs are written only after
success. Generic handle release and context/thread ownership rules apply.

Bond terms are captured by an unpriced structural snapshot sharing the exact
original cashflows and Settings. Source bond engine/cache/freeze changes remain
independent. Swap pricing retains the live index forwarding curve, discount
curve, fixings and Settings. Silent flow/fixing policy changes invalidate the
swap directly. Freeze pins cached values; explicit recalculate reprices while
frozen. Distinct Settings allocations or mixed Go sessions are rejected.

See the [Go binding contract](https://github.com/benbenbang/libitofin/blob/main/docs/go-binding-contract.md) for session/thread,
Close and pointer requirements. This bounded facade adds no core pricing math.

## Numerical evidence

The `asset_swap_native.tsv` fixtures are independently generated scalar outputs
from the separately installed QuantLib 1.43 Python wheel. They are not copied
QuantLib source tests and do not identify that wheel with the local 1.43-dev
source revision. Eight profiles cover fixed/zero bonds, payer reversal, negative
rates, scaled notional, nonpar redemption and upfront inclusion/exclusion.
Behavior tests also check signed BPS, fair-spread reconstruction and lifecycle.
