# Independent sequence-statistics references

These fixtures support [#1166](https://github.com/benbenbang/libitofin/issues/1166).
They contain no itofin-generated expected values.

## Reproduce

```sh
python sdk/go/testdata/sequence-statistics/generate.py \
  --quantlib /path/to/QuantLib --boost-include /path/to/boost/include
```

- Requires Python's standard library, a C++17 compiler and Boost headers.
- `--quantlib` must be a checkout of commit
  `9863b578af0caa4cecabf697196533e84a8308b6`.
- `--cxx` selects the compiler; `--output /tmp/replay/oracle.json` supports
  independent regeneration. Compilation uses a fresh, automatically removed
  temporary directory. No QuantLib checkout or shared build is changed.
- On this macOS host, regeneration used
  `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk`.
- The generator verifies the pinned bytes of each listed source before compiling
  `native.cpp`, `ql/errors.cpp` and `ql/math/statistics/generalstatistics.cpp`.
  The actual `QuantLib::SequenceStatistics` template supplies all eight outputs.
- `oracle.json` records compiler identity, source/generator hashes and the ordered
  individual case files. Each case records inputs, optional weights, `native`
  outputs and an independent `centered` reference. Matrices are row-major flat.

## Independent calculation

The centered reference uses exact Python `Fraction` arithmetic for weighted
means and centered products, then converts to binary64. Square roots use the
standard-library `math.sqrt`. Covariance uses `N/(N-1)` and counts zero-weight
rows in `N`. Min/max include all rows, including zero-weight ones.

Correlation is one on the diagonal and between two zero-variance columns,
zero between one constant and one varying column, otherwise covariance divided
by the two standard deviations. The generator verifies all eight native outputs
against the independent reference on the seven ordinary cases at absolute and
relative tolerance `2e-12`.

| Case | Purpose |
| --- | --- |
| `unit_two_columns` | Unit weights and nonperfect positive correlation |
| `unequal_weights` | Three columns, unequal weights and signed covariance |
| `zero_weight_extrema` | Ignored moment contribution but counted row/extrema |
| `two_constant_columns` | Four columns; both-zero and one-zero conventions |
| `single_column` | One-dimensional vector and 1-by-1 matrices |
| `negative_correlation` | Perfect negative correlation |
| `zero_weight_count` | Count correction differs after adding zero-weight row |
| `large_offset_cancellation` | Intentional stable covariance divergence |

For the last case, centered covariance is
`[5/3, 10/3, 10/3, 20/3]` and correlation is `[1, 1, 1, 1]`.
Native raw-product subtraction on the recorded Apple clang build instead returns
covariance `[-178956970.66666666, 0, 0, 0]` and correlation `[-1, 0, 0, 1]`.
Its lifted scalar variance remains `[5/3, 20/3]`. Those raw native defects are
recorded, not treated as correct stable outputs or widened-tolerance evidence.
The exact centered reference defines the deliberate numerical correction.
