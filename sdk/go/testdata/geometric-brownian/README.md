# Standalone arithmetic GBM oracle

`generate.py` compiles actual QuantLib `GeometricBrownianMotionProcess`,
`StochasticProcess1D` and `EulerDiscretization` from pinned commit
`9863b578af0caa4cecabf697196533e84a8308b6` (`1.43-dev`). It does not use a
Python wheel, installed QuantLib headers, or expected values computed by itofin.

```sh
python sdk/go/testdata/geometric-brownian/generate.py \
  --quantlib /path/to/pinned/QuantLib \
  --boost-include /path/to/boost/include \
  --build-dir /path/to/task-owned/native-build
```

The build extracts the exact Git archive without changing the checkout, compiles
its support source closure and records compiler-discovered upstream source/header
SHA256s in `oracle.json`. The current reduced build uses macOS dead stripping.
Two clean builds reproduce the fixture bytes. Only system C++ libraries are linked.

## Contract and independent check

- Nine cases cover signed/zero initial states, negative drift, negative states,
  negative shocks, zero volatility, zero time increment and crossing zero.
- `cases.json` and `cases.csv` contain the same full-precision native outputs.
  CSV is a dependency-free Rust consumer projection, generated rather than edited.
- Native drift is `mu*x`, diffusion is `sigma*x`. Expectation is
  `x+(mu*x)*dt`; variance is `(sigma*x)^2*dt`; signed standard deviation is
  `(sigma*x)*sqrt(dt)`; evolution adds that signed deviation times `dw`.
- The generator separately evaluates these elementary arithmetic identities.
  Comparisons require finite results and `max(2e-14, 3e-12*abs(expected))`.
- Upstream does not expose mu/volatility getters: constructor inputs are their
  independent oracle. This process is not exact-lognormal `SimulateGBM`; Euler
  can cross zero. Invalid-input/overflow rejection is an itofin safety contract,
  not a claim that upstream constructors reject the same inputs.

Upstream implementation notices and the [QuantLib license](https://www.quantlib.org/license.shtml)
remain external and unchanged. The standalone process credits Ferdinando
Ametrano, Sadruddin Rejeb and StatPro Italia.
