# FX-forward native fixtures

`generate.py` requires the PyPI **QuantLib 1.43** wheel. It calls the compiled
`FxForward` and `DiscountingFxForwardEngine` directly, using EUR/USD currencies,
fixed-reference `FlatForward` curves with Actual/365 Fixed, an evaluation date
of 2026-06-15 and a NullCalendar. The printed tuples are embedded in
`tests/medium2_fx_forward.rs`; no Python installation is needed for Rust tests.

```sh
python generate.py
```

The twelve vectors cover zero/two/ten-day spot lag, delivery on spot settlement,
unequal and future curve reference dates, negative interest rates, distinct rate
scales, equal-rate cancellation and tiny/large notionals. Native settlement days
match this port's explicit fixed spot-settlement date **only for this evaluation
snapshot**. Long means receive EUR/pay USD: native `paySourceCurrency=false`.
Both fair rate and **target-currency NPV** come from compiled engine methods.

The inspected original source is separately pinned at
`9863b578af0caa4cecabf697196533e84a8308b6`:
`ql/instruments/fxforward.{hpp,cpp}` and
`ql/pricingengines/forward/discountingfxforwardengine.{hpp,cpp}`. The wheel is
version 1.43, not a claim that it was built from that source revision.

This is numerical-contract validation, not full native instrument parity:
rolling settlement calendars, source-currency NPV, Instrument/pricing-engine
integration and a live spot quote are intentionally outside this additive API.
Rust also rejects invalid/non-finite values and non-representable intermediate
amounts more strictly than native code. Fixed absolute cash tolerance 1e-9
applies to ordinary-size vectors; huge notionals use relative tolerance 2e-15.
Tiny notionals are also checked relatively in the Rust test, avoiding an
uninformative absolute cash bound.
