# Seasonality consistency oracle

`seasonality_consistency.csv` records 17 outcomes checked by the independent
`seasonality_consistency.cpp` driver against QuantLib 1.43 at commit
`9863b578af0caa4cecabf697196533e84a8308b6`. The Rust integration test consumes
those same inputs and expected outcomes. The C++ driver calls QuantLib itself;
it does not reproduce the consistency algorithm.

The factor anchor is 31 January 2007. Factors default to one; `overrides` gives
semicolon-separated `index:value` pairs. Curve nodes are the first of the named
base month and 13 August 2012, with rates 2% and 3% and Actual/365 Fixed.

Cases distinguish stationary/daily shortcuts, two and three years, a failure
only in the third year, the curve's inflation-period end, a base date before
the factor anchor, calendar years crossing a leap day, and the strict `1e-5`
boundary in both directions, and a non-finite factor. The zero factor in the boundary case isolates exact comparison;
QuantLib permits it, and this case queries consistency, not a corrected rate.

With QuantLib built under `QuantLib/build`, run from the repository root:

```sh
c++ -std=c++17 -I QuantLib/build -I QuantLib \
  crates/libitofin/tests/fixtures/seasonality_consistency.cpp \
  -L QuantLib/build/ql -lQuantLib -Wl,-rpath,"$PWD/QuantLib/build/ql" \
  -o /tmp/seasonality-consistency
/tmp/seasonality-consistency crates/libitofin/tests/fixtures/seasonality_consistency.csv
cargo test -p libitofin --test seasonality_consistency
```

Add the Boost include directory if it is outside the compiler's default search
path (for example, `-I /opt/homebrew/include` on Apple Silicon Homebrew).
The driver exits nonzero for any outcome differing from the CSV.
