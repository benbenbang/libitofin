# Native callable-zero fixtures

Run `python generate.py` with actual QuantLib 1.43. The script refuses other
versions and writes UTF-8/LF `reference.tsv`. No libitofin implementation or
copied Rust result is used to generate expected values.

- Native object: `CallableZeroCouponBond`, `HullWhite`, and
  `TreeCallableFixedRateBondEngine`, 120 event-aligned tree steps.
- Reference/evaluation: 2026-10-12, TARGET settlement plus two business days.
- Face: 1,234,567.89. Contractual maturity: Sunday 2031-10-12.
- Issue: 2026-10-09. Actual/365 Fixed. Hull-White a=0.073, sigma=0.014.
- Continuous linear zero curve: reference/2027-10-12/2030-10-12/2035-10-12
  rates 0.015/0.031/0.024/0.04; negative-rate case uses flat -0.012.
- Twelve cases distinguish calls, puts, clean/dirty equivalence, mixed unsorted
  dates, coincident direction ordering, maturity calls, preceding redemption
  puts, settlement filtering, zero redemption and negative rates.
- The Rust comparisons use absolute 2e-8 currency and 2e-12 price tolerances.

Native 1.43's discretized callable bond indexes exercise directions from the
unfiltered original schedule. Mixed-direction expired events can therefore
misprice. Native settlement fixtures use consistent directions; separate
Rust argument and pricing regressions check correctly aligned mixed-direction filtering.

Checked source oracle: repository QuantLib commit
`9863b578af0caa4cecabf697196533e84a8308b6`, callablebond.cpp and
existing tree/discretized callable engines. Native package provenance is
separate from that source checkout and is not claimed to be the same build.
