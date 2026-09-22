# QuantLib 1.43 Heston oracle

`oracle.csv` is independent C++ QuantLib output from `oracle.cpp`: 13 c1-c4
rows, 78 nonzero-frequency complex characteristic-function rows and 42 prices across all six CV choices, default/fixed scaling and valid
alpha values. Cumulants use the upstream `testCosHestonCumulants` market. The
explicit asymptotic price uses v0=0.01, kappa=0.5, theta=0.01, sigma=2, rho=0,
where that variate applies; the other modes use the cumulant market.

Run `generate_macos.py /path/to/QuantLib` with the QuantLib 1.43 Python
interpreter, or compile against a built QuantLib 1.43 and execute the program. On
macOS the QuantLib Python wheel also exports the C++ symbols: compile a bundle
with `clang++ -std=c++17 -bundle -undefined dynamic_lookup`, both QuantLib and
Boost include paths (vendored headers identify themselves as 1.43-dev), then load `_QuantLib.abi3.so` using `ctypes.RTLD_GLOBAL`
and assert the Python wheel version is exactly 1.43 before calling the bundle's
`main` through ctypes. Capture stdout into a temporary file without rounding;
require successful exit and exactly 13 `c`, 78 `f` and 42 `p` rows before
replacing the fixture. The Rust test independently asserts all three counts.
No Rust implementation participates in generation.

`heston_engines.rs` also preserves the four COS cached values (1e-10), the
88 exponential-fitting extreme-moneyness values (1e-8), and truncation bounds
(1e-7) from `QuantLib/test-suite/hestonmodel.cpp`. The DAX 104-helper acceptance
runs all three engines in the core calibration tests with SSE 177.2 +/- 1.0.
Python/Go share their module-local fixtures in `sdk/go/testdata/`; their separate
control-variate generator requires QuantLib 1.43.
