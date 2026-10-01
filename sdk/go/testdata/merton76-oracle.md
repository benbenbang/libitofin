# Merton76 oracle provenance

Generated 2026-10-02 using native QuantLib 1.42 (PyPI macOS arm64 abi3 wheel).
Pinned checkout `9863b578af0caa4cecabf697196533e84a8308b6` is 1.43-dev; its
Merton76 process, jump engine and jump test sources equal tag v1.42
(`94db32184363113c4a53b0f98df2878299c96c49`). No libitofin outputs generate fixtures.
The original C++ adapter calls the exported native engine omitted by the Python
wrapper. QuantLib headers remain external; no binaries or environments are tracked.

`merton76-oracle.json` contains 12 native price/Greek cases, including Poisson
mean above 800 and a negative-jump put whose strike tail has mean 10 while its
adjusted asset tail has mean 0.0674. Independent original-intensity mixtures
agree within 4.3e-9; absolute native tolerance is 1e-8. Five-point independent
price derivatives validate all ordinary Greeks within 1e-6: spot bump 0.01,
rate/volatility/time bumps 0.0001. Vega is per diffusion-volatility unit; rho
and dividend rho are per continuous-rate unit; theta is per year, negative
maturity derivative. Gamma uses a five-point second spot derivative.

Default clocks: reference 2026-10-02, expiry plus `maturity_days`, Actual360,
continuous flat rates, NullCalendar. Clock variant 1: volatility reference
30 days earlier with Actual360, risk-free/dividend references unchanged with
Actual365Fixed. Conditional rate/volatility clocks follow upstream, while the
dividend clock remains its own. This variant's Greeks use native values.

`merton76-haug.csv` preserves all 135 corrected upstream Haug prices at original
absolute 1e-2 tolerance. Conversion: log-jump volatility is
`total_vol*sqrt(gamma/intensity)`, diffusion volatility is
`total_vol*sqrt(1-gamma)`, log-jump mean is `-jump_vol^2/2`, maturity is
`round(years*360)` days. Haug's 11-term truncation misses several corrected rows.

`merton76-edge-cases.json` separates independent zero-volatility limits from
upstream NaN theta/vega, and the nonzero tail hidden behind zero initial payoffs
(upstream wrongly returns zero). Zero-volatility derivative checks use an even
extension. Evaluation budgets must remain hard bounds, and convergence must
account for both asset and strike tails. Degenerate prices use independent
finite differences instead of accepting upstream NaNs as expected behavior.

Reproduce from the repository using Python with `QuantLib==1.42`, a C++17
compiler and Boost headers (`BOOST_INCLUDE`); macOS adapter command follows:

```sh
oracle=sdk/go/testdata
build_dir=$(mktemp -d)
mkdir -p "$build_dir/headers"
git -C QuantLib archive v1.42 ql | tar -x -C "$build_dir/headers"
printf '#define QL_USE_STD_ANY 1\n#define QL_USE_STD_OPTIONAL 1\n' > "$build_dir/headers/ql/config.hpp"
c++ -std=c++17 -shared -undefined dynamic_lookup -I"$build_dir/headers" -I"$BOOST_INCLUDE" "$oracle/merton76_oracle.cpp" -o "$build_dir/merton76_oracle.dylib"
python "$oracle/generate_merton76_oracle.py" --bridge "$build_dir/merton76_oracle.dylib"
```

Sources retain the [QuantLib license](../../../QuantLib/LICENSE.TXT), copyright
Ferdinando Ametrano, Sadruddin Rejeb and StatPro Italia. The table is attributed
by QuantLib's `test-suite/jumpdiffusion.cpp` to E. G. Haug, *Option Pricing
Formulas*, McGraw-Hill 1998, page 9; corrected values come from that test suite.
