# Bates independent numerical fixtures

- Generator: [`generate_bates_oracle.py`](generate_bates_oracle.py).
- Outputs: [`bates-oracle.json`](bates-oracle.json), 52 prices and 20 helper quotes.
- Rust price rows: [`bates-prices.tsv`](bates-prices.tsv), generated from the same cases.
- Oracle: QuantLib-Python **1.43**, no libitofin import or implementation output.
- Source review: QuantLib checkout **9863b578af0caa4cecabf697196533e84a8308b6**.
  The installed wheel is version-pinned, not certified as a build of that commit.
- Dates, clocks, parameter order, integration orders and bounds are in the fixture.

## Regenerate

In an isolated environment with `QuantLib==1.43` installed:

```sh
python sdk/go/testdata/generate_bates_oracle.py
```

The script requires QuantLib plus the Python standard library only. QuantLib is
an oracle-regeneration dependency, not a runtime requirement of libitofin.
The original local macOS ARM64 `_QuantLib.abi3.so` SHA-256 was
`04a53b6f425e728083149241f77e9b9711db845614ce65d2544a09a0970597d8`.
Float serialization can vary slightly across platforms; numerical bounds, not
byte-identical regenerated JSON, define the regression contract.

## Price evidence

| Cases | Independent calculation | Bound |
| --- | --- | --- |
| 4 named upstream markets | BatesEngine, order 160, 2007-03-30 to 2012-03-30 | Absolute 2e-10 |
| 18 flat-market calls/puts | BatesEngine, order 144, 90/365/730 days and 3 strikes | Absolute 2e-10 |
| 18 zero-intensity calls/puts | AnalyticHestonEngine, Gatheral, order 144 | Absolute 2e-10 |
| 6 high-intensity calls/puts | BatesEngine, order 160, negative risk-free rate | Absolute 2e-10 |
| 6 zero-jump-volatility calls/puts | Poisson-weighted Heston Gatheral prices, order 144 | Absolute 2e-10 |

The zero-intensity pins do not construct a zero-intensity QuantLib BatesModel:
its constructor has strictly positive jump-intensity and jump-volatility
constraints. libitofin intentionally admits zero for those two parameters.

For deterministic jump sizes, each Poisson count changes spot to
`S * exp(-lambda*T*expm1(nu) + count*nu)` before Heston pricing. The independent
mixture sums 128 terms and verifies Poisson mass to 1e-14. It does not call
BatesEngine. A separate research check against QuantLib's unchecked
`setParams(delta=0)` path agreed within 1.1e-14; that bypass is not the generator.

Finite quadratures are part of each case. The difficult Kahl-Jaeckel case differs
by about 1.28e-7 between upstream orders 160 and 192; do not compare different
orders and loosen the bound. No Monte Carlo or finite-difference acceptance
bands are reinterpreted as analytic-engine tolerances.

## Calibration evidence

- Flat curves: 3% risk-free, 1% dividend, spot 100, Actual365Fixed, NullCalendar.
- Helpers: 3/6/12/24 months, strikes 80/90/100/110/120, RelativePriceError.
- Quotes: independent Bates order-144 prices inverted through QuantLib helpers
  with 1e-12 volatility accuracy; the fixture stores prices and implied vols.
- Parameter order: `[theta, kappa, sigma, rho, v0, nu, delta, lambda]`.
- Fit 1 fixes all except intensity: start lambda 1.2, target 0.7.
- Fit 2 fixes the five Heston parameters and fits all three jump parameters:
  start `[-0.06, 0.25, 0.3]`, target `[-0.12, 0.18, 0.7]`.
- LM tolerances and end criteria are recorded per fit, including the tighter
  1e-12 setting for the three-parameter fit. All fixed parameters must stay fixed.
- Acceptance: each fitted parameter within 1e-6 of the independent target;
  maximum helper relative-price error below 1e-8. Optimizer endpoints are not
  required to match bit for bit. Original wheel residuals are recorded.

These are identifiable synthetic fits, not the upstream DAX fit. The latter uses
104 market quotes, a nonflat zero curve and ImpliedVolError; its original test
accepts squared implied-vol residuals times 10,000 of `36.6 +/- 2.5`. Neither that
market nor its original acceptance band is silently replaced by a flat curve.

## Primary sources and attribution

- [Named Bates markets and DAX calibration test](https://github.com/lballabio/QuantLib/blob/9863b578af0caa4cecabf697196533e84a8308b6/test-suite/batesmodel.cpp),
  `testAnalyticVsMCPricing`, `testDAXCalibration`.
- [Bates characteristic jump exponent](https://github.com/lballabio/QuantLib/blob/9863b578af0caa4cecabf697196533e84a8308b6/ql/pricingengines/vanilla/batesengine.cpp).
- [Parameter ordering and constraints](https://github.com/lballabio/QuantLib/blob/9863b578af0caa4cecabf697196533e84a8308b6/ql/models/equity/batesmodel.cpp).
- [Helper option selection and date clock](https://github.com/lballabio/QuantLib/blob/9863b578af0caa4cecabf697196533e84a8308b6/ql/models/equity/hestonmodelhelper.cpp).

QuantLib sources are BSD-3-Clause. The named upstream input cases derive from its
Bates tests, copyright Klaus Spanderen (2005, 2008) and StatPro Italia (2007).
The generator is original fixture code; the numerical oracle remains QuantLib.
