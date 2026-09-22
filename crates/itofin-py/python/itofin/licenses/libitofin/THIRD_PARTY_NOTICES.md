# QuantLib Heston engines

The COS engine, cumulant expressions, AP control-variate extensions and
exponentially fitted quadrature engine/table are adapted from QuantLib 1.43:
`ql/pricingengines/vanilla/{coshestonengine,analytichestonengine,exponentialfittinghestonengine}.cpp`.

Copyright (C) 2017, 2020 Klaus Spanderen; Copyright (C) 2022 Ignacio Anguita.
Upstream files retain their additional notices. The complete QuantLib license
and contributor notices are included in [QUANTLIB_LICENSE.txt](QUANTLIB_LICENSE.txt).

The table preserves every source decimal token. Reproduce it with
`scripts/generate_heston_fitting_table.py QuantLib --check` from the repository
root. Its generated header records the full source SHA-256.
