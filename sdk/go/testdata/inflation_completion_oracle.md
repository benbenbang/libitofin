# Inflation completion oracle

`inflation_completion_oracle.cpp` independently reproduces the helper dates and
piecewise curve nodes in `TestInflationCompletionHelperDates` and
`TestInflationCompletionPiecewiseDetachedOutputs`. It links C++ QuantLib, without
loading libitofin. The fixture uses UK RPI history for April-July 2007, flat 2.95%
inflation swap quotes, a 5% nominal curve, and a two-month observation lag.

Verified against the repository's QuantLib checkout at
`9863b578af0caa4cecabf697196533e84a8308b6` (`1.43-dev`). Build that checkout first,
then run from the repository root, with the Boost include directory available:

```sh
c++ -std=c++17 -O2 -IQuantLib -I"$BOOST_INCLUDE" \
  sdk/go/testdata/inflation_completion_oracle.cpp \
  -LQuantLib/build/ql -lQuantLib -Wl,-rpath,"$PWD/QuantLib/build/ql" \
  -o /tmp/itofin-inflation-completion-oracle
/tmp/itofin-inflation-completion-oracle
```

Helper case IDs are flat, linear with an early-month observation, linear with a
late-month observation, and linear with an explicit maturity pillar. Both zero
and YoY helpers return respectively `(pillar, latest)` of June 1/June 1,
June 1/July 1, July 1/July 1, and July 1/July 1, all in 2008.

Both curves have July 1, 2007; June 1, 2008; and June 1, 2009 nodes. Their
30/360 Bond Basis times from August 13, 2007 are independently computable as
`-42/360`, `288/360`, and `648/360`. YoY node rates are all `0.0295`.
Zero node rates are `0.026250783298904422`, `0.026250783298904422`, and
`0.027944745304696286`. The Go test preserves an absolute `1e-12` tolerance.

Slice mutation and snapshots after handle closure are Go ownership assertions;
QuantLib's oracle provides the numerical and calendar expectations. Index
representations use the existing Go `String()` method and public `Name()` and
`Ratio()` metadata, matching Python representation semantics without adding APIs.
