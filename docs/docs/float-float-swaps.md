# Plain float-float swaps

`FloatFloatSwap` is an additive Rust-core instrument for a same-currency basis
swap: pay the first Ibor leg and receive the second. Both legs have one constant
positive nominal, but may have different schedules, indexes, spreads and accrual
day counters. No principal is exchanged.

## Valuation

- Import `libitofin::instruments::FloatFloatSwap` and the `Instrument` trait.
- Construct with nominal, first schedule/index/spread/day counter, second
  schedule/index/spread/day counter, optional payment convention and Settings.
- Attach the existing `DiscountingSwapEngine` through `base_mut()`.
- Use the same Settings instance for indexes, swap and engine.
- An omitted payment convention uses each schedule's own convention.
- Engine settlement and NPV date choices keep their existing behavior.

`npv()` is signed paid PV plus signed received PV. `pay_leg_npv()` and
`pay_leg_bps()` use the paid sign; received-leg measures use the received sign.
For a positive nominal and positive discounts, paid BPS is negative and received
BPS is positive. BPS uses each coupon's actual accrual convention, even when it
differs from the index day counter.

`fair_pay_spread()` and `fair_receive_spread()` return **absolute spreads**, not
spread shifts. Each holds the other leg fixed and solves:

```text
fair spread = current spread - NPV / (signed leg BPS / 0.0001)
```

This is exact for the plain uncapped, unit-gearing slice. Zero or non-finite
sensitivity is an error, including an expired leg. An expired swap has zero NPV
and zero leg measures.

## Updates and errors

Quote updates, curve relinks, fixing changes and evaluation-date moves follow
the existing observer/cache machinery. Non-notifying Settings flags are checked
on queries and invalidate the cache silently when changed. Generic instrument
freeze/unfreeze behavior is preserved; ordinary reads do not broadcast updates.

Invalid nominal/spreads, malformed schedules, different or blank index
currencies, mismatched index Settings and day-tenor/overnight indexes are
rejected. Missing fixings, unlinked curves, missing engines and curve-domain
failures remain errors, not zero values. A valid evaluation date is required.
Empty forwarding handles can be linked later.

## Oracle and boundaries

Tracked whole-instrument QuantLib 1.43 fixtures cover signed PV/BPS, fair-spread
repricing, negative rates, differing coupon day counters, settlement cutoffs,
Settings today-flow overrides, non-reference NPV dates, payment-convention
resolution and expiry. Fixture generation imports QuantLib, not this Rust core.
PV/BPS tolerances are absolute `2e-9`; fair-spread tolerance is `2e-13`.

Not included: unequal or varying notionals, principal exchanges, payer/receiver
variants, non-unit gearing, caps/floors, CMS/CMS-spread coupons, in-arrears or
specialized pricing engines. There are no new C, Python or Go bindings and no
changes to existing instrument APIs or defaults.
