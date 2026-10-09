# Black-Scholes theta observations

- Source oracle: QuantLib checkout at
  `9863b578af0caa4cecabf697196533e84a8308b6`, `ql/pricingengines/greeks.cpp`.
- Compiled oracle: independent PyPI `QuantLib==1.43`. This wheel is not claimed
  to have been built from the inspected checkout revision.
- The wheel does **not** export `blackScholesTheta`. Four vanilla fixtures compare
  its compiled `AnalyticEuropeanEngine` theta against the inspected PDE identity
  applied to compiled value/delta/gamma and current market queries.
- Live updates, nonflat zero curves and an explicit local-volatility override
  observe compiled process inputs and then evaluate the inspected source law in
  Python. These are not direct compiled helper calls.
- Tests embed numeric observations and do not need Python, network access or an
  external JSON file. They use absolute tolerance `3e-10`, allowing the existing
  short-time zero-rate conversion to differ from analytic-engine flat rates by
  floating-point roundoff.

Reproduce an audit JSON artifact, with an environment containing QuantLib 1.43:

```sh
python crates/libitofin/tests/fixtures/medium_bs_theta/generate.py \
  --source-root /path/to/QuantLib \
  --output /tmp/medium-bs-theta.json
```

The generator verifies both versions and records the inspected source SHA256.
Rust-only tests additionally cover spot-dependent local volatility, relinks,
empty handles, invalid quote reads, non-finite inputs, zero/negative volatility,
overflow and preservation of the existing theta-per-day IEEE behavior.
