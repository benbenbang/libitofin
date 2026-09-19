# Kerkhof seasonality reference

`kerkhof_seasonality.cpp` generates `kerkhof_seasonality.csv` directly with
QuantLib. It does not import or compute libitofin results. The checked fixture
was generated against the repository's QuantLib native library
`libQuantLib.1.43.0.dylib`; its source header identifies itself as `1.43-dev`.

```sh
c++ -std=c++17 -IQuantLib -I/path/to/boost/include \
  crates/libitofin/tests/fixtures/kerkhof_seasonality.cpp \
  -LQuantLib/build/ql -lQuantLib -Wl,-rpath,"$PWD/QuantLib/build/ql" \
  -o /tmp/kerkhof-seasonality
/tmp/kerkhof-seasonality > crates/libitofin/tests/fixtures/kerkhof_seasonality.csv
```

The 72 rows cross January, July and December anchors with monthly and quarterly
curves, Actual/365 Fixed and 30/360 Bond Basis, leap-day queries, dates before the
curve base, and multi-year dates. Dates use QuantLib serial numbers. The factor
vector is specified in both the generator and Rust test. The tolerance is
`1e-13`, with exact classification of infinite outputs. Before-base curve
queries are rejected even with extrapolation; `nan` in the curve column denotes
that rejected query, while direct correction remains defined. Sixty-two rows
distinguish Kerkhof from multiplicative price seasonality by more than `1e-6`.
The generator also verifies rejected factor counts and unsupported YoY rates.

## Source and consumer contract

- `ql/termstructures/inflation/seasonality.hpp:170-187` defines the constructor
  and virtual overrides. The C++ constructor fixes the initial frequency to
  Monthly. Rust exposes exactly twelve monthly factors and validates this
  contract at construction and atomic replacement, earlier than C++'s factor
  lookup. Arbitrary-frequency mutation inherited from the C++ parent is omitted.
- `seasonality.cpp:220-256` multiplies factor indices from the lower calendar
  month up to, but excluding, the higher month. Month numbers are one-based;
  factor element zero is unused. A backwards month interval uses the reciprocal.
  Days and years are ignored, including across December/January.
- `seasonality.cpp:258-277` uses this factor directly, raised to the reciprocal
  year fraction from the start of the curve-base month. It does not divide by
  the curve-base factor. YoY correction throws. Factor values are not checked
  for positivity or finiteness, normalized, or constrained to an annual product.
- Inherited `correctZeroRate` (`seasonality.cpp:123-133`) quantizes the query to
  the curve's inflation period start. Inherited consistency (`:64-88`) accepts
  the stationary twelve-factor specification without a product or alignment
  check. Therefore consistency does not imply that YoY correction is supported
  or that a misaligned base month produces a finite zero-time correction.
- `InflationTermStructure::setSeasonality` installs through `Seasonality` and
  notifies; `ZeroInflationTermStructure::zeroRate` folds the correction into date
  queries. Time queries retain raw interpolated rates. Zero inflation index
  forecasts and swap helpers consume those corrected date queries.
- Rust interpolated curves accept `Shared<dyn Seasonality>` in their constructor
  and `set_seasonality`. Piecewise curves invalidate and rebootstrap after
  replacement or removal. The focused bootstrap test uses the existing
  independent fourteen-swap fixture, demands changed nodes and forecasts, checks
  repricing with its original `1e-7` NPV tolerance, and verifies restoration after
  clearing. Ownership retains the correction while installed; mutation through
  a shared object is not provided. Replace through the curve to notify consumers.

The original multiplicative implementation and its tolerances are unchanged.
Multi-year and arbitrary-frequency Kerkhof variants and additive seasonality are
not supported.
