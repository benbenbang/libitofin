# Go behavioral test triage (#1005)

Baseline review: `4c48647e`, macOS arm64, Go 1.27.1. The coverage audit's
744 mapped symbols minus 578 explicit test references yields 166, comprising
93 containing types inferred from mapped members, 46 methods and 27 enum members.
A missing reference is not evidence that the corresponding code never executes.
The 111 newer unmapped RNG/calendar symbols belong to #1003/#1004.

## Types (93)

All 93 are containing types inferred by `scripts/check_go_coverage.py` from
mapped members. Constructors, members, aliases and returned objects establish
these types in compiled Go tests. Separate type-name assertions would add no
behavioral evidence. This does not claim every member or configuration is tested.

## Methods (46)

Names below omit `itofin.`; term-structure names are in `termstructures`.
"Deferred" means no numerical/behavioral gate is claimed for that specific API.

| API group | Count | Evidence or concrete follow-up |
| --- | ---: | --- |
| `indexes.ZeroInflationIndex.__repr__, link_to`; `indexes.YoYInflationIndex.__repr__` | 3 | Deferred: assert representation metadata; relink zero index and verify a forecast against an independently flat inflation curve after old handles close. YoY relinking already has a separate test. |
| `instruments.CreditDefaultSwap.accrual_rebate_date, calculate, fair_upfront, notional, price` | 5 | Existing NPV/lifecycle tests exercise calculation indirectly. Deferred: assert direct methods, rebate settlement date and zero-NPV fair-upfront reconstruction. |
| `instruments.MakeCreditDefaultSwap.__init__, build` | 2 | Deferred: compare builder schedule/cashflows and cached NPV with an explicit standard CDS fixture. Both Python calls map to one Go factory. |
| `DefaultProbabilityHelper.latest_date, pillar_date` | 2 | Deferred: pin CDS helper dates around IMM/holiday boundaries from QuantLib. |
| `DefaultProbabilityTermStructure.default_density, default_density_date, default_probability, default_probability_date, hazard_rate_date, survival_probability` | 6 | Added `TestFlatHazardAnalyticQueriesAndRetainedQuote`: independent exponential survival/density formula, date/time equivalence, quote mutation, retained dependencies and closed-session error. |
| `InterpolatedHazardRateCurve.dates, hazard_rates` | 2 | Nodes and terminal survival already asserted by `TestCreditCurveNodesAndSessionIsolation`; deferred direct array content/copy-independence assertions. |
| `InterpolatedYoYInflationCurve.dates, nodes, times` | 3 | Date-rate behavior already checked; deferred ordered output, day-count times and detached-array assertions. |
| `InterpolatedZeroInflationCurve.dates, nodes` | 2 | Times/date/time rates already checked; deferred direct dates/nodes and detached-array assertions. |
| `MultiplicativePriceSeasonality.frequency, seasonality_base_date` | 2 | Factors/seasonal application already checked; deferred exact metadata round trip. |
| `PiecewiseDefaultCurve.__init__, calculate, data, dates, nodes, times`; `SpreadCdsHelper.__init__` | 7 | Deferred high priority: bootstrap multiple quoted CDS tenors, independently reprice quotes, verify pillars and update/release quote dependencies. |
| `PiecewiseYoYInflationCurve.nodes, times` | 2 | Bootstrap dates, forecasts and quote-driven relinking already checked; deferred direct node/time output. |
| `PiecewiseZeroInflationCurve.dates, times` | 2 | Bootstrap rates/update and retained nodes already checked; deferred direct date/time output. |
| `YoYInflationHelper.latest_date, pillar_date`; `ZeroInflationHelper.latest_date` | 3 | Zero pillar/observation dates already checked; deferred remaining exact helper dates. |
| `YoYInflationTermStructure.base_date, frequency, yoy_rate` | 3 | Date rates checked; deferred metadata and analytical time interpolation between explicit nodes. |
| `ZeroInflationTermStructure.base_date, frequency` | 2 | Base time/rates checked; deferred exact metadata. |

## Enum members (27)

| Group | Count | Evidence or concrete follow-up |
| --- | ---: | --- |
| `CpiInterpolationType.Flat, Linear` | 2 | Existing inflation helper tests assert different pillar dates; direct metadata test also checks Flat. |
| `PricingModel.Isda, Midpoint` | 2 | Isda implied-hazard recovery directly checked. Midpoint pricing engine checked, but deferred Midpoint implied-hazard dispatch. |
| `ProtectionSide.Buyer, Seller` | 2 | Seller cached CDS NPV checked; deferred Buyer/Seller sign reversal for identical contracts. |
| `SettlementMethod.CollateralizedCashPrice, ParYieldCurve, PhysicalCleared`; `SettlementType.Cash`; `CashAnnuityModel.DiscountCurve` | 5 | Deferred: supported cash/physical swaption fixtures and discounted-annuity valuation oracle, plus explicit rejection where unsupported. |
| `SwapType.Receiver` | 1 | Deferred payer/receiver sign reversal and fair-rate consistency. |
| `AccrualBias.HalfDayBias, NoBias` | 2 | Existing ISDA test checks default HalfDayBias and explicit NoBias coupon cached values. |
| `ForwardsInCouponPeriod.Flat, Piecewise`; `NumericalFix.NoFix, Taylor` | 4 | Existing ISDA test checks explicit Piecewise/Taylor against defaults; deferred Flat/NoFix non-flat-curve oracle. |
| `BondPriceType.Clean, Dirty` | 2 | Clean fixture reconstructs a discount analytically; deferred Dirty with nonzero accrued interest. |
| `FuturesType.Asx, Custom, Imm` | 3 | IMM repricing/default end date and missing Custom end-date rejection checked. Deferred valid Custom and ASX maturity/pricing cases. |
| `Pillar.LastRelevantDate, MaturityDate` | 2 | Existing zero-inflation helper test checks default LastRelevantDate and explicit MaturityDate. |
| `RateAveraging.Compound, Simple` | 2 | Compound is exercised through the default OIS helper bootstrap; deferred explicit Simple versus Compound numerical oracle. |

## Calibration variants and defaults

The old PriceError/ImpliedVolError references proved only native enum validation.
`TestHullWhiteCalibrationErrorVariantsAndDefaults` calibrates both variants on
the five-swaption `testCachedHullWhite` fixture and checks fitted parameters and
signed helper residuals against independent QuantLib output. It repeats with
omitted and explicit optimizer/end-criteria defaults. The parameter tolerance is
the existing cached-oracle tolerance, 1.3e-5. Heston calibration with these two
error types, fixed-reversion variants and broader optional-argument combinations
remain deferred; this slice does not close #1005 by itself.

## Validation provenance

The Linux results in `go-bindings-review.md` are historical, not rerun here.
Local macOS arm64 results (Go 1.27.1, Rust 1.96.0):

- `cargo build -p libitofin-ffi --release`: passed.
- `cargo test -p libitofin-ffi --release models_api::tests::calibration_enum_and_optional_criteria_are_checked`: one passed.
- `GOEXPERIMENT=cgocheck2 go test -race -count=1 -run 'TestHullWhiteCalibrationErrorVariants|TestFlatHazardAnalytic'` in `bindings/go`, with `DYLD_LIBRARY_PATH` pointing to this worktree's release output: both top-level tests passed, including both calibration variants.
- `python3 scripts/check_go_coverage.py --strict --baseline`: 744/744 baseline mapped, 584 explicit test references, no invalid references; 111 newer symbols still unmapped.

The independently downloaded QuantLib 1.43 wheel generated the calibration
constants using [the reproducible oracle](../bindings/go/testdata/hullwhite_calibration_oracle.py).
Its fixture follows QuantLib `test-suite/shortratemodels.cpp:testCachedHullWhite`,
changing only the error metric. Signed residuals use 1e-8 absolute tolerance;
parameters retain 1.3e-5. The constant-hazard test uses `exp(-hazard * time)`
and its analytical derivative, with 1e-12 tolerance.
No remote CI or private consumer integration is claimed.
