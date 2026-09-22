# Optionlet stripping references

- Runtime: QuantLib Python wheel 1.43; vendored C++ headers: 1.43-dev.
- `extract.py PATH/TO/QuantLib/test-suite/optionletstripper.cpp` reproduces the
  original 16x13 normal/lognormal matrices and two zero curves unchanged.
  Source SHA256: `93b240cd61430e4056b48511e89639ab75f5bf645e2cda4f5b610318741e238b`.
- Inputs originate in QuantLib's `CommonVars` and `CommonVarsON`, under the
  [QuantLib license](https://www.quantlib.org/license.shtml). Copyright holders
  are recorded in the vendored source; the extracted market data retain that attribution.
- Original nonflat cap-price and floating-switch tests retain `2.5e-8` absolute
  tolerance and stripping accuracy `1e-6`. Both indexed and at-par switch values
  are checked. No Rust-generated expected prices are used.
- `oracle.cpp` supplies independent nonzero Stripper2 corrections and the
  off-node cubic normal smile. Compile as a macOS bundle using `clang++ -std=c++17
  -bundle -undefined dynamic_lookup`, QuantLib and Boost include paths; load the
  wheel extension with `ctypes.RTLD_GLOBAL`, then call `optionlet_oracle()`.
- `overnight_oracle.py PATH/TO/QuantLib` generates the actual overnight-leg cap
  price and three surface values. It uses the original source market data and
  explicitly sets a quarterly coupon schedule. The upstream test leaves its
  schedule tenor default-initialized and compares two adapters over the same
  stripper; neither is used as evidence of numerical equivalence here.

The overnight stripper intentionally reproduces upstream's Ibor upcast: daily
Ibor cap premiums are differenced into the chosen output frequency. It can
produce very large stripped normal volatilities and cap prices. This is exact
reference behavior, not a claim that it equals a conventionally stripped OIS
surface. The actual cap consumer uses retained compounded overnight coupons,
last fixing dates, payment adjustment and payment lag.
