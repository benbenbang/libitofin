# Lazy zero-inflation base-date oracle

`generator.cpp` calls independent QuantLib `1.43-dev`, commit
`9863b578af0caa4cecabf697196533e84a8308b6`. Its first assertion ports
`test-suite/inflation.cpp:512-593` (`testZeroTermStructureLazyBaseDate`): construct
before quotes/fixings exist, populate the 14 swap quotes and 31 UK RPI fixings,
then require exact equality of the lazy and fixed base dates and node vectors.

`oracle.csv` adds five sets of all 15 nodes plus the August 2012 index forecast:

- Initial market at 13 August 2007, latest fixing July 2007.
- Clear/repopulate the history with the July fixing corrected to 208.1.
- Move evaluation to 13 September and publish August at 208.4.
- Move evaluation to 13 October without adding a fixing.
- Raise the first helper quote by five basis points.

Each phase changes the solved nodes. Rust compares independent node rates at
`1e-12`, forecasts at the upstream inflation pricing tolerance `1e-7`, and helper
repricing at `1e-12`. The original lazy/fixed node equality remains exact. Date
serials follow QuantLib's date convention. Every phase builds a fresh independent
fixed-base curve and records its complete date/rate grid and forecast. The driver
also requires persistent lazy-curve rates and forecasts to agree at these same
tolerances. On the base-date advance, QuantLib's fixed-reference bootstrap keeps
its old node-zero date (`iterativebootstrap.hpp:230`) even though `baseDate()`
advances. `persistent_node_date` records that cached grid separately. Rust retains
its existing bootstrap rebuild behavior and compares all dates/rates directly to
the fresh QuantLib curve. No Rust output is used to generate
expected values.

Build and regenerate from the repository root, adjusting dependency paths:

```sh
c++ -std=c++17 -I QuantLib/build -I QuantLib -I /opt/homebrew/opt/boost/include \
  crates/libitofin/tests/fixtures/lazy_inflation_base/generator.cpp \
  -L QuantLib/build/ql -lQuantLib -Wl,-rpath,"$PWD/QuantLib/build/ql" \
  -o /tmp/lazy-inflation-base-generator
/tmp/lazy-inflation-base-generator > \
  crates/libitofin/tests/fixtures/lazy_inflation_base/oracle.csv
cargo test -p libitofin --test lazy_inflation_base
```

The Rust constructor owns the callback and resolves it before each bootstrap
(`piecewisezeroinflationcurve.hpp:73-90,128-132,166-171`). Helper notifications
invalidate the result; arbitrary callback dependencies require `update()`.
`with_last_fixing_date` owns an unlinked index copy sharing settings and fixing
history, observes fixing/date updates, and cannot retain its forecast curve.
`try_base_date()` propagates missing inputs and bootstrap errors. The existing
infallible `base_date()` returns a null date on lazy failure; pricing and new
binding inspectors use the fallible path. The fixed-base constructor is unchanged.

Only linear interpolation is constructible. Foreign-language callbacks are not
exposed; binding coverage is limited to the concrete latest-fixing constructor.
Callbacks retaining their own consumers still require weak captures or explicit
cycle breaking. The concrete constructor does not have that ownership cycle.
