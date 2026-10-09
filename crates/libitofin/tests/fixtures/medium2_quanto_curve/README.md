# Quanto curve native fixtures

- Inspected original QuantLib source revision:
  `9863b578af0caa4cecabf697196533e84a8308b6`.
- Independent compiled reference: PyPI `QuantLib==1.43`, a distinct revision.
- The wheel exports `QuantoTermStructure`. The generator calls its compiled
  `discount` and `zeroRate` methods, not a reimplementation of the formula.
- It records 24 observations across three scenarios: common clocks/strike100,
  mixed reference dates/day counters/strike140, and mixed clocks/strike60.
  The latter two asset strikes lie outside the surface's `[80,120]` domain.
- All scenarios use nonflat zero curves, a strike-sensitive variance surface,
  and a nonflat FX variance curve. Times include zero, `1e-8`, `1e-4`, interpolation
  points and extrapolation beyond the wrapper's maximum calendar date.
- Native values are baked into `tests/medium2_quanto_curve.rs`; Rust tests never
  import Python. Discount absolute tolerance is `2e-14`. Zero-rate tolerance is
  `2e-12` except at `1e-8`, where only discounts are compared: dividing a rounded
  discount logarithm by tiny time magnifies normal floating-point roundoff.

Regenerate outside the tracked tree with the pinned source checkout:

```sh
python generate.py --source-root /path/to/QuantLib --output /tmp/quanto-native.json
```

The output identifies source revision/hash, wheel version and observed API.
Generator replay is deterministic on the designated compiled reference.
