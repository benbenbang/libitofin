# Default-probability jump oracle

`credit_jumps_oracle.cpp` runs against QuantLib 1.43. The small derived hazard
curve supplies the same 2% continuous intensity as `FlatHazardRate`; QuantLib's
base owns all jump behavior. Its stock flat-curve constructors omit jumps.

Contract: `ql/termstructures/defaulttermstructure.cpp:64-100` generates December
31 dates once, updates their times when the reference date changes, and multiplies
the consecutive quote factors whose jump times are strictly less than the query.
Applied quotes must be valid and in `(0, 1]`. Density and quoted hazard remain
unscaled (`ql/termstructures/credit/hazardratestructure.hpp:106-108`).

Regenerate from the repository root with the pinned QuantLib build:

```sh
c++ -std=c++17 -IQuantLib -I/opt/homebrew/opt/boost/include \
  sdk/go/testdata/credit_jumps_oracle.cpp -LQuantLib/build/ql -lQuantLib \
  -Wl,-rpath,"$PWD/QuantLib/build/ql" -o /tmp/credit-jumps-oracle
/tmp/credit-jumps-oracle > sdk/go/testdata/credit_jumps_oracle.csv
```

Adjust the Boost include path for the host. Rust tests consume all 12 rows at
`1e-15` absolute tolerance. Separate analytic tests cover errors, constructor
length/date validation, input ordering, quote ownership and cached CDS repricing.
