# L2 discrepancy oracle

## Independent references

- QuantLib source: `9863b578af0caa4cecabf697196533e84a8308b6`.
- `native.cpp` executes the actual `DiscrepancyStatistics`.
- Seeded point sets come from actual pinned `SobolRsg` with **Jaeckel**
  direction integers and `MersenneTwisterUniformRng`, seed **1729**. These are
  not SciPy Sobol sequences. First Sobol point is the native first returned
  point; no all-zero point is prepended.
- Four hand-authored cases include closed-cube corners, duplicates, interior
  points and one 3D point. Four generated cases cover 2D/8 and 3D/16 points.
- `generate.py` converts every binary64 coordinate into exact `Fraction`
  values, evaluates an independent ordered double sum and computes
  `sqrt(float(exact_squared))`. It never calls itofin.

For dimension `d`, count `N`, points `x_i`:

```text
A = sum_i sum_j product_k (1 - max(x_ik, x_jk))
C = sum_i product_k (1 - x_ik^2)
D^2 = A/N^2 - 2^(1-d)*C/N + 3^(-d)
```

`oracle.json` records verified pinned source hashes, native/generator hashes
and actual compiler identity/flags. Each fixture records its exact rational
squared result and independently checked native result.

## Domain and limitations

Native dimension one is rejected, verified by an executable probe. Its zero
constructor does not establish a valid positive dimension and is not used as
an oracle: itofin requires dimension 2..256; reset(0) on an initialized Rust
accumulator preserves the existing dimension. Points must be finite in the
closed cube `[0, 1]^d`. Weights are omitted or exactly one per point; general
weighted discrepancy is not implemented even though the native API accepts
weights. Storage is O(Nd), work O(N²d). Limits are 4,096 rows, 1,000,000
coordinates and 100,000,000 pair-coordinate operations.

This uncentered L2 point-set coverage diagnostic is not SciPy's centered
`CD` default, a Monte Carlo confidence interval or an integration-error bound.

## Regenerate

```sh
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk \
python3 sdk/go/testdata/discrepancy-statistics/generate.py \
  --quantlib QuantLib --build-dir /private/tmp/discrepancy-oracle
```

Pass `--output-dir /private/tmp/discrepancy-check` for byte comparison with
committed JSON. Boost headers default to `/opt/homebrew/include`.
