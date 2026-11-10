# OptionletStripper2 references (QuantLib v1.44)

- Source: QuantLib v1.44 C++ (`optionletstripper2.cpp`), CMake Release build (`-O3 -DNDEBUG`),
  macOS arm64, Apple clang; the program itself is built with `-std=c++17 -O2`. QuantLib-Python 1.44
  does not expose `OptionletStripper2`, so the values come from the C++ library.
- `stripper2_v144.cpp` prints `case index atm_strike atm_price spread` per ATM tenor
  (1Y, 3Y, 5Y) with `std::setprecision(17)` into `oracle.txt`.
- Market: evaluation date 2013-10-28, flat 4% Actual365Fixed TARGET curve, Euribor6M,
  cap tenors 1, 2, 3, 5, 7, 10Y, strikes 1, 2, 4, 6, 10%, OptionletStripper1 accuracy 1e-10.
  - `lognormal`: flat 18% surface, ATM 20% (the QuantLib 1.43 `optionlet_stripping/oracle.cpp` case).
  - `lognormal_below`: flat 18% surface, ATM 17% (negative spread bracket).
  - `displaced`: flat 18% surface, displacement 1%, ATM 20%.
  - `normal`: normal surface `.008 + .0002 i + .002 k`, ATM 0.85%, 0.95%, 1.00%.
  - `steep`: surface `.05 + .05 i` by tenor, ATM 1%: the 3Y ATM price is below the minimum
    attainable price, so the line records the C++ error message.
- Regenerate, with QuantLib v1.44 built into `QL_BUILD` and its source in `QL_SRC`:

```
c++ -std=c++17 -O2 -I$QL_BUILD -I$QL_SRC -I$(brew --prefix boost)/include stripper2_v144.cpp \
  -L$QL_BUILD/ql -lQuantLib -Wl,-rpath,$QL_BUILD/ql -o stripper2_v144
./stripper2_v144 > oracle.txt
```

- The market data follow QuantLib's test suite under the
  [QuantLib license](https://www.quantlib.org/license.shtml).
