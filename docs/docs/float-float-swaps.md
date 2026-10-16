# Plain float-float swaps

`FloatFloatSwap` is an additive instrument for a same-currency basis
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
specialized pricing engines. Existing instrument APIs and defaults are unchanged.

## Python and Go bindings

Python exposes `itofin.instruments.FloatFloatSwap`. Go exposes
`Session.NewFloatFloatSwap(FloatFloatSwapConfig{...})`, with separate paid and
received schedules, indexes, spreads and day counters. The C ABI uses a dedicated
`ItofinFloatFloatSwapConfig` and typed handle, not the existing generic swap kind.

Python constructor order is nominal, paid schedule/index/spread/day count,
received schedule/index/spread/day count, Settings, then optional keyword-only
`payment_convention`. In Go the corresponding config fields are `PaySchedule`,
`PayIndex`, `PaySpread`, `PayDayCounter` and the `Receive` equivalents, plus
`Nominal`, `Settings` and optional `PaymentConvention`.

Attach the live discount curve with Python `swap.set_engine(curve, settings)`
or Go `swap.SetEngine(curve, settings)`. This direct attachment matches the
existing vanilla-swap style and preserves native default settlement/NPV dates
and flow policy. The Settings argument must be the exact same allocation used
by both indexes and the swap. A same-valued independent Settings object is
rejected before engine replacement. No optional date overrides are exposed.

- Python: `npv`, `pay_leg_npv`, `receive_leg_npv`, `pay_leg_bps`,
  `receive_leg_bps`, `fair_pay_spread`, `fair_receive_spread`.
- Go: `NPV`, `PayLegNPV`, `ReceiveLegNPV`, `PayLegBPS`, `ReceiveLegBPS`,
  `FairPaySpread`, `FairReceiveSpread`.
- Both retain native calculate/recalculate, freeze/unfreeze, calculated-state and
  expiry controls. Missing engines or fixings and unlinked curves remain errors.
- Curve/index/Settings dependencies remain live after their external wrappers
  are released. Released swap handles still reject further access normally.
- Retained checked calendars guard coupon payment/fixing construction and live
  forecast traversals; finite calendar horizons are not silently extrapolated.

```python
from itofin.instruments import FloatFloatSwap

swap = FloatFloatSwap(
    1_000_000.0,
    paid_schedule, paid_index, 0.001, paid_day_count,
    received_schedule, received_index, -0.002, received_day_count,
    settings,
)
swap.set_engine(discount_curve, settings)
value = swap.npv()
absolute_paid_spread = swap.fair_pay_spread()
```

The dedicated binding fixture tests use the unchanged QuantLib 1.43 oracle's
`base`, `negative`, `following` and `expired` profiles where discount reference,
engine settlement and NPV-date defaults match this public contract. The core's
other explicit settlement/NPV-date profiles are not claimed by this facade.

A separately generated QuantLib 1.43 direct-default fixture covers six live
reference-date/today inclusion policies with the discount reference on the
payment/evaluation date. These tests reuse one cached instrument to verify that
silent Settings updates are read without manual invalidation. Runnable examples:
`example/python/float_float_swap.py` and `sdk/go/examples/float-float-swap`.
