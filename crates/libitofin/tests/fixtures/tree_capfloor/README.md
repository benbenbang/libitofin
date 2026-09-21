# Hull-White cap/floor tree oracle

Run `generator.py` using QuantLib 1.43. Its six independent NPVs are retained in
`pricingengines/capfloor/treecapfloorengine_tests.rs` with absolute tolerance
`1e-8`. The fixture covers a cap, floor and collar at 30 and 100 target steps,
non-unit gearing, nonzero spread and ACT/360 coupon accruals.

Separate tests exercise fixed grids, known historical fixings, invalid inputs,
model updates and error recovery. The tree uses Hull-White; other short-rate
models are outside this engine's API.

Normal-volatility helpers use 2/3/4/5-year tenors, flat 3%, quoted 1% normal
volatility and omit the first swaplet. Initial market/model values retain
`1e-12` tolerance; Levenberg-Marquardt fits sigma with mean reversion fixed
and matches QuantLib within `1e-8`. Tree refinement from 30 to 600 steps
reduces the cap error against the analytic engine below `1e-3` relative.

`binding_oracle.py` uses the exposed unit-gearing IborLeg facade and supplies
the Python/Go literal prices (`1e-11`) and
vegas (`1e-10`), including negative rates and zero volatility. Same-day intrinsic,
non-finite inputs and recovery are separately exercised in Rust.
