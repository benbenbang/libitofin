# Overnight-Ibor basis oracle

`generator.py` uses the actual helper in the public QuantLib 1.43 Python wheel.
Run `python generator.py > quantlib.csv` with that version installed. Columns:
mode, basis, tenor years, spot serial, maturity serial, pillar serial, discount.
Modes 0/1 fit Ibor against a flat overnight curve with omitted/explicit discount.
Mode 2 couples an OIS curve discounted on Ibor with an Ibor curve fitted to basis
quotes against that OIS curve. Both curves use log-linear discounts.

QuantLib's Python wrapper has no `MultiCurve`. The independent oracle instead
alternates complete bootstraps using immutable prior Ibor discount snapshots
until both curves' discounts change by less than 2e-15. Each basis scenario
converges in seven iterations (joint residuals 1.11e-16 and 5.55e-16). It asserts
1e-12 relative helper repricing and 1e-10 absolute independent swap NPVs. Rust
uses the actual circular `MultiCurve` and compares these discounts at 1e-12
relative, before and after a live basis change.

Source: `ql/experimental/termstructures/basisswapratehelpers.cpp:121-198`.
The executable implementation always fits Ibor, applies basis to the overnight
leg, and defaults discounting to fitted Ibor. The upstream header's prose says
otherwise for default discounting. No `bootstrapBaseCurve` option exists.
`piecewiseyieldcurve.cpp` multi-curve tests cover Ibor-Ibor, not this sibling;
their 1e-10 Boost percent quote tolerance is 1e-12 relative, retained here.
Python/C/Go exposure of this helper is deferred; the #1066 binding consumer
covers the Ibor-Ibor helper only.
