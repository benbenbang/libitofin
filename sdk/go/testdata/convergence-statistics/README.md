# Mean-convergence oracle

## Independent references

- QuantLib source: `9863b578af0caa4cecabf697196533e84a8308b6`.
- `native.cpp` executes the actual `ConvergenceStatistics<GeneralStatistics>`.
- `generate.py` separately computes weighted prefix means with exact Python
  `Fraction` arithmetic. Neither reference calls itofin.
- Checkpoints are **1, 3, 7, 15, ...**, from executable
  `DoublingConvergenceSteps`: `next = 2 * current + 1`. The upstream prose
  describing powers of two is not the implemented rule.
- Six cases cover unit/unequal weights, zero-weight observations after a
  positive first observation, constant values, an incomplete final checkpoint
  and an empty table. Empty `mean: null` denotes an unqueried native mean.
- `oracle.json` records source/generator/native SHA-256 hashes, actual compiler
  identity and flags. Every pinned source is compared against its Git blob.

## Explicit divergence

The native zero-first probe accepts `add(7, 0)`: sample count becomes one and
its first table mean is NaN, without throwing. The manifest records this
actual behavior. Itofin rejects a zero-total checkpoint **without mutation**.
Other input errors are atomic too; inputs must be finite and weights finite
and nonnegative. No checkpoint is synthesized for an incomplete prefix.

Only the default doubling rule and scalar mean diagnostic are exposed. The
100,000-observation cap bounds retained data; this is not the full upstream
statistics decorator, a convergence certificate or an error estimate.

## Regenerate

Run from the repository root, selecting a compatible SDK if needed:

```sh
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk \
python3 sdk/go/testdata/convergence-statistics/generate.py \
  --quantlib QuantLib --build-dir /private/tmp/convergence-oracle
```

Use `--output-dir /private/tmp/convergence-check` for an independent byte
comparison with the committed JSON files. Boost headers default to
`/opt/homebrew/include`; override with `--boost-include` when necessary.
