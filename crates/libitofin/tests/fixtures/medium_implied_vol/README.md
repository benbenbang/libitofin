# Implied Black variance reference observations

- Inspected QuantLib source pin: `9863b578af0caa4cecabf697196533e84a8308b6`.
- Compiled reference: PyPI `QuantLib==1.43`, a distinct revision.
- The wheel does **not** expose `ImpliedVolTermStructure`. The generator uses
  compiled original curves' `blackForwardVariance` with the inspected C++
  rebasing and variance-to-volatility laws. It does not claim to execute the
  compiled implied wrapper.
- Nonflat curves, a relink to a different reference date/day counter, and a
  strike-dependent surface provide 19 observations. Surface rebasing is a
  numerical parity check, not a financial recommendation.
- `medium_implied_vol.rs` bakes the observations, with no runtime Python,
  QuantLib or JSON dependency. Additional Rust tests cover guards, moving
  reference dates, zero/tiny-time adaptation and weak observer ownership.

Run with compiled `QuantLib==1.43` installed:

```sh
python generate.py --source-root /path/to/pinned/QuantLib \
  --output /tmp/implied-vol-evidence.json
```

The optional JSON audit artifact records the source revision and file hashes,
inputs, compiled numerical route and explicit limitations. It is not a test
input. The source checkout's pin must match exactly.
