# BMA municipal swap oracle

`oracle.cpp` adapts QuantLib's `testBMACurveConsistency` and its ten `bmaData`
quotes from `test-suite/piecewiseyieldcurve.cpp` (QuantLib license; copyright
2000-2007 RiskMap, StatPro Italia and contributors). Evaluation date is fixed at
2025-10-23; previous BMA fixing is 3% on 2025-10-22. USD Libor 3M forecasts and
discounts with a 4% continuous Actual/360 curve referenced to settlement.

`oracle.json` and `swaps.csv` were independently generated with QuantLib 1.43.
`oracle.json` is copied to `sdk/go/testdata/bma.json` for installed Go modules.
Core tests preserve upstream iterative tolerance 1e-9 and local-bootstrap
1e-6, including the monotone natural-spline zero-rate configuration.
Cached swap outputs use 1e-9 and coupon rates 1e-12.

The headers identify themselves as 1.43-dev; execution uses the released 1.43
Python wheel's exported C++ symbols. On macOS, compile `oracle.cpp` as a bundle
with `-std=c++17 -bundle -undefined dynamic_lookup`, adding QuantLib and Boost
include paths. Load `QuantLib._QuantLib` via `ctypes.CDLL(..., RTLD_GLOBAL)`, then
load the bundle and call `bma_oracle()`. Capture stdout as JSON. No Rust result
is used to generate these fixtures.

Run `generate.py` with the independent QuantLib interpreter. It checks version
1.43, builds the bundle and rewrites both JSON copies and the Rust CSV fixture.
