# Third-party notices

## QuantLib Heston engines

The COS engine, cumulant expressions, AP control-variate extensions and
exponentially fitted quadrature engine/table are adapted from QuantLib 1.43:
`ql/pricingengines/vanilla/{coshestonengine,analytichestonengine,exponentialfittinghestonengine}.cpp`.

Copyright (C) 2017, 2020 Klaus Spanderen; Copyright (C) 2022 Ignacio Anguita.
Upstream files retain their additional notices. The complete QuantLib license
and contributor notices are included in [QUANTLIB_LICENSE.txt](QUANTLIB_LICENSE.txt).

The table preserves every source decimal token. Reproduce it with
`scripts/generate_heston_fitting_table.py QuantLib --check` from the repository
root. Its generated header records the full source SHA-256.

## QuantLib Merton jump diffusion

The Merton76 process and European pricing engine are adapted from QuantLib's
`ql/processes/merton76process.{hpp,cpp}` and
`ql/pricingengines/vanilla/jumpdiffusionengine.{hpp,cpp}`.
Copyright (C) 2001-2003 Sadruddin Rejeb; Copyright (C) 2003, 2004 Ferdinando
Ametrano; Copyright (C) 2004, 2005, 2007 StatPro Italia srl.
The complete QuantLib license and contributor notices are included above.
Cached Haug test values retain the attribution to E. G. Haug, *Option Pricing
Formulas*, McGraw-Hill 1998, page 9, from QuantLib's `test-suite/jumpdiffusion.cpp`.

## QuantLib GJR-GARCH

The daily-parameter model, analytic Edgeworth expansion and European Monte
Carlo conventions are adapted from QuantLib's `ql/models/equity/gjrgarchmodel`,
`ql/pricingengines/vanilla/{analyticgjrgarchengine,mceuropeangjrgarchengine}`
and `test-suite/gjrgarchmodel.cpp`. Copyright (C) 2008 Yee Man Chan.
The complete QuantLib license and contributor notices are included above.
The original DAX calibration data retain QuantLib's attribution to A. Sepp.
Source pins, binary provenance and fixture reproduction are recorded in
`sdk/go/testdata/gjrgarch-model-oracle.md`.

## Independent global optimizers

The bundled `itofin-optimize` DE and PSO solvers and their calibration adapters
are independently written, not adapted from QuantLib or another optimizer.
PSO follows Kennedy and Eberhart (1995), DOI
<https://doi.org/10.1109/ICNN.1995.488968>, and Shi and Eberhart (1998), DOI
<https://doi.org/10.1109/ICEC.1998.699146>. DE follows Storn and Price (1997),
DOI <https://doi.org/10.1023/A:1008202821328>. The crate's own notices document
our policies and prior design comparisons; no clean-room claim is made.

## QuantLib currency presets

The twenty additional currency presets and their six-field test fixtures are
adapted from QuantLib revision
`9863b578af0caa4cecabf697196533e84a8308b6` (local version 1.43-dev),
`ql/currencies/{africa,america,asia,europe,oceania}.cpp`.
The stored name, code, numeric code, symbol, fraction symbol and fractions per
unit are preserved. QuantLib rounding and triangulation behavior are not included.
The exact source hashes and field provenance are recorded in
`docs/docs/currency-presets.md` and the accompanying currency preset TSV fixtures.

The upstream source headers contain these notices:

- `africa.cpp`: Copyright (C) 2004, 2005 StatPro Italia srl;
  Copyright (C) 2016 Quaternion Risk Management Ltd;
  Copyright (C) 2023 Skandinaviska Enskilda Banken AB (publ).
- `america.cpp`: Copyright (C) 2004, 2005, 2008 StatPro Italia srl;
  Copyright (C) 2016 Quaternion Risk Management Ltd.
- `asia.cpp`: Copyright (C) 2004, 2005 StatPro Italia srl;
  Copyright (C) 2016 Quaternion Risk Management Ltd.
- `europe.cpp`: Copyright (C) 2004, 2005, 2006 StatPro Italia srl;
  Copyright (C) 2016 Quaternion Risk Management Ltd.
- `oceania.cpp`: Copyright (C) 2004, 2005 StatPro Italia srl.

The complete QuantLib license and contributor notices are included in
[QUANTLIB_LICENSE.txt](QUANTLIB_LICENSE.txt).
