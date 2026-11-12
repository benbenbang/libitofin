# Ziggurat Gaussian seed-42 oracle

`seed42_draws.tsv` records draws 0..100 of
`ZigguratGaussianRng<Xoshiro256StarStarUniformRng>` seeded with 42, as printed by
`seed42_draws.cpp` against QuantLib v1.44 (tag `v1.44`). Each row has the index,
the value at `std::setprecision(17)` and its IEEE-754 bit pattern.

- Header: `ql/math/randomnumbers/zigguratgaussianrng.hpp` at `v1.44`, which
  carries the corrected Marsaglia-Tsang wedge acceptance.
- Build: QuantLib v1.44 compiled as a shared library on macOS (arm64).
- No row is a tail draw (every `|x|` is below `normR = 3.654...`), so no
  returned value goes through `log` or `exp`. `exp` still enters the wedge
  accept/reject comparison (index 95 is a wedge acceptance), which leaves the
  values themselves exact products of the tables and the raw draws.
- Before the v1.44 wedge fix the Rust stream first differs at index 95.

Regenerate, with `QL_SRC` the v1.44 source tree and `QL_BUILD` its build tree.
Get the source without checking out the pinned clone, then build the library
(Release, shared) with CMake; `BOOST_INC` is the Boost include directory
(`$(brew --prefix boost)/include` on macOS, `/usr/include` on Debian/Ubuntu):

```sh
git -C QuantLib archive v1.44 | tar -x -C "$QL_SRC"
cmake -S "$QL_SRC" -B "$QL_BUILD" -DCMAKE_BUILD_TYPE=Release && cmake --build "$QL_BUILD" --target ql_library
c++ -std=c++17 -O2 -I"$QL_BUILD" -I"$QL_SRC" -I"$BOOST_INC" \
  crates/libitofin/tests/fixtures/ziggurat_gaussian_v144/seed42_draws.cpp \
  -L"$QL_BUILD/ql" -lQuantLib -Wl,-rpath,"$QL_BUILD/ql" -o /tmp/seed42-draws
/tmp/seed42-draws > crates/libitofin/tests/fixtures/ziggurat_gaussian_v144/seed42_draws.tsv
cargo test -p libitofin --lib zigguratgaussianrng
```
