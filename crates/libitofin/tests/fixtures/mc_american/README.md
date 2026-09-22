# MC-American depth oracles

Generated independently with QuantLib 1.43. `prices.json` is copied unchanged to
`sdk/go/testdata/mc_american_depth.json` for Python/Go consumers.

- Run `scripts/fixtures/mc_american/quantlib.py` in an environment with QuantLib 1.43.
  American fixtures reproduce `test-suite/mclongstaffschwartzengine.cpp`'s six
  markets. The five newly ported cases retain FD price band `2.34 * error` and
  exercise-probability tolerance `0.015`. The existing first case retains its
  explicitly documented cached-MC reference exception.
- Compile `scripts/fixtures/mc_american/basis.cpp` against QuantLib 1.43 and save
  stdout to `basis.csv`. All seven families at three states and orders 0-3 are
  checked at absolute `1e-13`; this discriminates families unlike upstream's
  `0 * (...)` basis index, which always selects Monomial.
- Bermudan dates are reference date + 91, 203, 365 days. The exercise-only grid
  uses three steps; the refined grid uses 75. Both price tests retain the
  `2.34 * error` band. The reference is QL MC for the exercise-only grid and
  QL FD (1600 time steps, 800 space points) for the refined grid.
- `upstream_unmasked_refined_mc` reproduces the upstream LSM defect: intermediate
  simulation dates become exercise opportunities. Rust deliberately masks them
  in both calibration and pricing. A deterministic path regression prevents
  accidental exercise between contractual dates.

The original fixture setup/probabilities are from QuantLib's test suite;
QuantLib copyright and license remain in `QuantLib/LICENSE.TXT`.
