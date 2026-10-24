# European gap native FD reference

`gap_fd_reference.csv` is an LF-only serialization of the independent QuantLib
1.43 wheel export. The immutable raw CRLF export has SHA256
`b323c65fbdcceb235dbbbd6a7e9ff7cbd3b332997d8136537aae968e5e54ae87`.
The tracked LF copy has SHA256
`2d90130bb09080fae340d4fcd2743102a49244aee950b25efce05b7fa4d461b3`.
Only CRLF became LF; every parsed header/data cell string is identical, with no
numerical or tolerance change. The Python fixture copy has identical LF bytes.
Actual360, evaluation/reference date
2026-06-15, explicit elapsed days, Douglas, time/equity grids 100/200/400/800 and
damping 0/2 are part of the recipe. No candidate implementation generated values.

There are 240 numeric rows: 192 ordinary positive-stock/trigger/volatility and
nonexpired rows, plus 48 finite edge-policy diagnostics. Ordinary public tests
use only the 192 rows. They require finite NPV/delta/gamma/theta and same-grid
native absolute agreement 1e-8 for NPV/delta/gamma, 1e-7 for theta, declared before
candidate execution. This is not closed-form convergence or blanket accuracy.

Closed-form NPVs are independent high-precision truncated-lognormal moments.
Native grid errors can be nonmonotonic and significant: negative put L=-20 has
about 0.07371 absolute error at grid800/damping2. Negative/cancelling terminal
cells retain the inherited Simpson-accuracy point-value fallback. Short-expiry
routes can worsen as the grid changes. No tolerance or core math was repaired.

Zero-vol-drift, zero-trigger and expiry-today rows are separate diagnostics, not
ordinary passing comparisons. Additional native errors (including zero-vol-flat
at trigger and nonpositive stock) are retained in the permanent QA edge record.
Inclusive native point payoff, analytic ATM half-mass and expired-instrument
policy are distinct contracts.

Source reference: QuantLib 9863b578af0caa4cecabf697196533e84a8308b6. The compiled
1.43 wheel is explicitly not claimed built from that exact source revision.
Permanent author/QA evidence retains exact generator, pure-I/O exporter,
independent laws, full native observations, provenance and edge/convergence
qualification metadata under `local/codex/binding-wave7-20261024/`. Native source
and compiled-wheel identities are recorded separately.
