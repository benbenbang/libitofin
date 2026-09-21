# SOFR futures oracles

QuantLib 1.43 generates `curves.csv` via `curve_oracle.cpp` and `accrual.csv` via
`generate.py`. The native generator uses Discount/Linear, which the Python wheel
does not expose. Compile against the vendored QuantLib headers/library, run with
stdout redirected to `curves.csv`; run the Python script with QuantLib 1.43.

Curve inputs and fixings exactly reproduce `sofrfutures.cpp` `testBootstrap` and
`testBootstrapWithJuneteenth`. Independent Rust futures retain the upstream 1e-9
price gates (97.44, convexity-adjusted 87.44, and 97.220). Juneteenth curve nodes
also match at 1e-10 absolute; every helper's price on the frozen native curve
matches at 1e-9.

The native local bootstrap itself leaves November/December 2018 monthly prices
at 97.768168528143036/97.68363222520351 versus 97.77/97.685 market quotes. Their
holiday-end daily forwards cross into later nodes. Rust's local bootstrap has
the same tail dependence, with node differences around 4e-9; node equality is
not asserted for that case. Frozen-curve pricing isolates instrument fidelity
without claiming those monthly quotes fully reprice after later nodes appear.

`accrual.csv` covers Simple/Compound, holiday starts/ends, a weekend evaluation
date, and optional today's fixing, at unchanged 1e-9 absolute. The curve is 3.5%
continuous Actual/365 Fixed; the generator records the past fixing schedule.

The SDK keeps byte-identical copies at `sdk/go/testdata/sofr_futures_*.csv` so
its packaged tests do not depend on the Rust workspace. Regeneration must update
both copies.
