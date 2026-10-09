# Hull-White forward-process oracle

- Original source checkout: QuantLib revision
  `9863b578af0caa4cecabf697196533e84a8308b6`.
- Compiled numerical oracle: PyPI `QuantLib==1.43`, a distinct wheel rather
  than a build of that checkout. `HullWhiteForwardProcess` is directly exposed.
- Source: `ql/processes/hullwhiteprocess.cpp`, SHA-256
  `c9bec96ecfe040e59e9664853b18566939b40c57d02b8a9d1c9dff633c5393ee`.

Reproduce with the task's QuantLib environment:

```sh
python generate.py --source-root /path/to/QuantLib \
  --output /tmp/hullwhite-forward.json --verify-rust
```

The generator checks its source pin, wheel version and source-law markers.
`--verify-rust` also requires exactly matching numeric values in the tracked
`NATIVE` and `LIMITS` constants after parsing Rust's array literals.

- Seven direct native cases cover flat and nonflat curves, time zero, signed
  states, zero volatility, and horizons before the process time. Each checks
  nine process quantities, with a shared `2e-12` absolute tolerance.
- Five 80-digit Decimal calculations independently check the integrated
  measure adjustment at zero/tiny mean reversion, with `5e-18` tolerance.
- The generator records native zero-`a` drift as nonfinite and tiny-`a`
  cancellation diagnostically. Those values are not accuracy oracles.
- Decimal also validates `B/1e16` at `a=1e-16`, duration `1e16`: the original
  epsilon shortcut is inaccurate there. Rust uses stable positive-`a` algebra.
- An independent Simpson integral checks an ordinary adjustment. Original
  Ornstein-Uhlenbeck variance's small-speed approximation remains unchanged.

No oracle needs network access after the pinned wheel is installed.
