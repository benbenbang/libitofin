# Swaption matrix constructor and Black-engine oracle

`quantlib.csv` is generated solely by `oracle.cpp` against QuantLib 1.43.
The evaluation date is fixed at 2026-06-15; all conventions and the 6-by-4
market grid come from `test-suite/swaptionvolstructuresutilities.hpp`.

The 120 `node` rows are `kind,form,option-index,swap-index,vol,npv,recovered`.
The five `observe` rows are `kind,form,initial,moved,bumped,relinked,0`.

| Form | Reference date | Market data | Option axis |
| --- | --- | --- | --- |
| 0 | Moving | Quote handles | Tenors |
| 1 | Fixed | Quote handles | Tenors |
| 2 | Moving | Numeric matrix | Tenors |
| 3 | Fixed | Numeric matrix | Tenors |
| 4 | Fixed | Numeric matrix | Explicit dates |

The observation query keeps its exercise date fixed while evaluation moves to
2025-06-15, then restores evaluation and bumps the first quote to 0.20 and
relinks it to a new quote at 0.30. Numeric matrix forms retain copied values.

Source map:

- `ql/termstructures/volatility/swaption/swaptionvolmatrix.hpp`: all five constructor shapes.
- `test-suite/swaptionvolatilitymatrix.cpp`, `makeCoherenceTest`: date/tenor/time
  nodes at 1e-16; ATM `MakeSwaption`, `EuriborSwapIsdaFixA` and
  `BlackSwaptionEngine` implied-volatility recovery at 1e-6. Rust additionally
  matches the independent NPVs at 1e-12. Since Rust has no instrument-level
  implied-volatility helper, its test solves a separate flat-quote Black engine
  with Brent, the same 0.98 initial guess, [1e-6, 4] bracket and 100 evaluations.
- `ql/indexes/swap/euriborswap.cpp`: 1Y swap uses Euribor 3M, longer swaps 6M.
- `makeObservabilityTest`: reference-date and live-market updates. The oracle
  also tests quote-handle relinks and actual numeric constructors; upstream's
  two cases labelled fixed market data currently pass quote handles instead.

Regenerate from the repository root with an independently built QuantLib:

```sh
c++ -std=c++17 -IQuantLib -I/path/to/boost/include \
  crates/libitofin/tests/fixtures/swaption_matrix/oracle.cpp \
  -LQuantLib/build/ql -lQuantLib -Wl,-rpath,"$PWD/QuantLib/build/ql" \
  -o /tmp/swaption-matrix-oracle
/tmp/swaption-matrix-oracle > crates/libitofin/tests/fixtures/swaption_matrix/quantlib.csv
```
