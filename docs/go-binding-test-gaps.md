# Go behavioral test evidence

[#1036](https://github.com/benbenbang/libitofin/issues/1036) completes the bounded
cases deferred by the original #1005 triage. Tests below use independent
QuantLib or analytical expectations; API mappings alone do not prove behavior.
This inventory does not claim exhaustive QuantLib product coverage.

## Completed cases

| Scope | Executable evidence |
| --- | --- |
| LSM basis selection and rejection, latest-only American, date-only Bermudan QuantLib oracles, retained inputs and quote recovery | `TestMCAmericanBasisAndLatestOnly`, `TestMCBermudanRetentionUpdatesAndErrors`, `TestMCBermudanQuantLibOracles` |
| COS/exponential Heston engines: four COS prices at `1e-10`, 88 fitted-quadrature prices at `1e-8`, six control variates, retained inputs, invalid options and live-model calibration | `TestCosHestonQuantLibRetentionAndErrors`, `TestExponentialFittingHestonQuantLibGrid`, `TestExponentialFittingHestonVariatesAndErrors`, `TestAlternativeHestonCalibrationAndLiveModel` |
| BMA ten-tenor QuantLib curve/swap/coupon values, weekly holidays, live quotes/history, recovery and cold retained ownership | `TestBMAQuantLibCurveSwapCouponAndHolidayOracles`, `TestBMAUpdatesHistoryRecoveryAndColdRetention`, `TestBMANilForeignSessionAndInvalidConstructors` |
| Normal non-flat optionlet stripping, independent ATM corrections, fixed/moving quote curves, off-node cubic smiles and retained adapters | `TestOptionletStripperCompletion` |
| Overnight stripping constructor/frequency and retained inputs, inversion fallback, foreign-session and nil rejection | `TestOptionletStripperOvernightAndFallback` |
| Actual compounded overnight cap prices with flat and stripped normal volatility, coupon counts and retained inputs | `TestOvernightCapFloorOracle` |
| Retained cap quote/date observations, moving maximum-date bounds and invalid-discount recovery with fallback enabled | `TestOptionletStripperRetainedCapRecovery` |
| Custom pillars: independent QuantLib helper dates and curve values, invalid bounds, retained inputs and date recovery | `TestCustomYieldPillarsQuantLibAndRecovery`, `TestCustomInflationPillarsQuantLib`, `TestCustomFraOverloadsAndSwapDiscount` |
| Overnight futures: both accrual conventions, holiday clipping, live convexity/fixings, expiry and released inputs at `1e-9` | `TestOvernightFutureQuantLibAccrualAndRetention`, `TestSofrFutureBootstrapAndCustomPillars` |
| Hull-White Bermudan tree, six indexed/par QuantLib cached prices, bumped-quote oracle, retained inputs and invalid-input recovery | [Tree swaption tests](../sdk/go/tree_swaption_test.go) |
| Iterative bootstrap signed bound widening, approximate fallback, evaluation limits, cached retry and retained inputs | `TestIterativeBootstrapQuantLibOracle`, `TestIterativeBootstrapAllYieldFactories`, `TestIterativeBootstrapRetryRetentionAndErrors`, `TestIterativeBootstrapInvalidOptions` |
| Mutually coupled Ibor basis curves, independent FRA/swap repricing, quote/discount/date/fixing updates and retained joint ownership | `TestJointYieldCurvesQuantLibRepricingAndUpdates`, `TestJointYieldCurvesFixingsAndCloseOrders`, `TestJointYieldCurvesInvalidInputs` |
| SABR backward-flat sparse/dense QuantLib oracle, quote/date recalibration, retained inputs and error recovery | `TestSABRBackwardFlatQuantLibOracle`, `TestSABRBackwardFlatUpdatesAndRetainedInputs`, `TestSABRBackwardFlatInvalidInputs` |
| Kerkhof monthly factors, independent zero corrections, copied factors, retained handles and YoY errors | `TestKerkhofSeasonalityQuantLibOracle`, `TestKerkhofSeasonalityOwnershipAndErrors` |
| Lazy zero-inflation base dates, independent nodes/forecasts, fixing/date/quote updates and retained dependencies | `TestLazyInflationBaseMatchesQuantLibAndRetainsDependencies` |
| Zero/YoY index representations and retained ratio identity | `TestInflationCompletionIndexRepresentations` |
| Piecewise zero/YoY dates, times, nodes and detached copies | `TestInflationCompletionPiecewiseDetachedOutputs` |
| Zero/YoY helper latest and pillar dates, flat/linear interpolation | `TestInflationCompletionHelperDates` |
| CDS rebate settlement, weekend handling and absent rebate | `TestCreditCompletionRebateSettlementBusinessDays` |
| CDS builder, explicit schedule, premium/default cashflows, cached calculation and fair-upfront zero NPV | `TestCreditCompletionBuilderCashflowsAndFairUpfront` |
| Midpoint implied hazard, ISDA dispatch and buyer/seller signs | `TestCreditCompletionMidpointHazardAndProtectionSigns` |
| Hazard dates/rates detached copies and non-flat ISDA NoFix/Taylor, Flat/Piecewise combinations | `TestCreditCompletionHazardArraysAndNonflatISDA` |
| Default-density interpolation, bootstrap repricing, quote updates and retained dependencies | `TestDefaultDensityInterpolationAndBoundary`, `TestDefaultDensityBootstrapLiveHelpers` |
| Survival-jump strict boundaries, date updates and retained quotes | `TestCreditJumpsQuantLibOracle`, `TestCreditJumpsInvalidArguments`, `TestCreditJumpDatesWithoutEvaluationDate` |
| ISDA spread/upfront helper repricing, quote updates, ownership and settings restoration | `TestIsdaCreditHelpersOracleAndRetention`, `TestCreditHelperExplicitTermsErrors` |
| CDS protection end independent of adjusted payment date | `TestCreditProtectionEndDate` |
| All five swaption matrix constructors, explicit dates, live quotes/settings, copied values and retained dependencies | `TestSwaptionMatrixFiveConstructorOracles`, `TestSwaptionMatrixAdditionalConstructorErrors` |
| Eonia OIS cached price, MakeSwaption calendar/forecast oracles, retained pricing and live volatility repricing | `TestEoniaOisSwaptionCachedLifecycle`, `TestMakeSwaptionCalendarAndForecastOracles` |
| Swaption cash/physical settlement, both annuity models, unsupported combinations, payer/receiver fair rates | `TestRatesCompletionSwaptionSettlementAndReceiver` |
| Dirty/clean bond quotes with nonzero accrued interest | `TestRatesCompletionDirtyBondWithAccruedInterest` |
| Valid Custom/ASX futures dates, prices and analytical forward/discount factors | `TestRatesCompletionCustomAndASXFutures` |
| Simple and Compound OIS numerical oracles | `TestRatesCompletionOISAveragingOracle` |
| OIS quote/date updates, separate discounting, forward starts, retained objects and invalid-enum recovery | `TestOvernightAveragingOISUpdatesAndRetention`, `TestOvernightAveragingOISForwardAndDiscounting`, `TestOvernightAveragingInvalidEnumDoesNotPoisonSession` |
| OIS today-to-history error recovery and live additive-spread rebootstrap | `TestOvernightTodayForecastRecoversAfterMissingPastFixing`, `TestOvernightAdditiveSpreadQuoteRebootstrapsExistingCurve` |
| Heston PriceError/ImpliedVolError fitted parameters and signed residuals | `TestHestonCalibrationPriceAndImpliedVolOracles` |
| Hull-White fixed reversion and mixed omitted/explicit optimizer and stopping settings | `TestHullWhiteCalibrationFixedReversionAndOptionalCombinations` |
| Normal cap/floor/collar prices and vegas, negative rates, zero volatility, tree prices and retained inputs | `TestCapNormalAndTreeOracles` |
| Normal CapHelper calibration, mandatory grids, live quotes and invalid-input recovery | `TestCapHelperNormalCalibrationAndRecovery` |
| Hull-White zero-start-delay calibration after input wrappers close, original QuantLib PAR cache at `1e-5` | `TestHullWhiteCachedNoStartDelayAfterInputsClosed` |

The CDS builder includes the final accrual day; the current explicit Go
constructor does not expose that convention. Each is checked against its own
matching QuantLib contract, and the one-day premium difference is reconstructed
analytically. Their NPVs are intentionally different.

[#1038](https://github.com/benbenbang/libitofin/issues/1038) adds the default
arithmetic-averaging overnight coupon path in Rust and successful Simple OIS
pricing through Python and C/Go. The default uses the full daily schedule and
zero convexity adjustment. Custom arithmetic-pricer volatility and telescopic
approximation are outside this increment. The retained QuantLib 1.43 discount/quote
tolerances are unchanged. The richer
[overnight oracle](../crates/libitofin/tests/fixtures/overnight_averaging/README.md)
covers all three language surfaces.

[#1045](https://github.com/benbenbang/libitofin/issues/1045) aligns Compound
forecast discount-ratio compounding, today's fixing enforcement and daily-spread
treatment with QuantLib, including partial forecast intervals. Both previously
excluded coupon cases now run. Exact enforcement and daily-spread switches remain
Rust-only; #808 additionally exposes overnight fixing-history writes in Python/Go.
The OIS tests cover exposed pricing and recovery from missing-history errors;
they do not claim coverage of unexposed switches.
[#1047](https://github.com/benbenbang/libitofin/issues/1047) registers the helper's
additive-spread handle with its existing observer, so a live spread change
invalidates and rebootstraps the same fitted curve in Rust, Python and Go.

## Oracle reproduction and tolerances

Sources and fixtures live in [`sdk/go/testdata`](../sdk/go/testdata/):

- [Optionlet stripping oracle](../crates/libitofin/tests/fixtures/optionlet_stripping/oracle.cpp):
  QuantLib 1.43 runtime with 1.43-dev headers; the companion `extract.py` preserves
  the vendored upstream non-flat matrices. Binding cap prices retain `2.5e-8`;
  independent ATM prices, strikes and spreads are pinned separately from cap
  repricing. The off-node cubic smile uses `1e-10`; spread estimates use `1e-7`.
  The vendored overnight test constructs both adapters from the same stripper,
  so that self-comparison is not evidence of overnight/Ibor equivalence. Separate
  actual overnight-cap prices use independent QuantLib expectations at `2.5e-8`;
  the [overnight generator](../crates/libitofin/tests/fixtures/optionlet_stripping/overnight_oracle.py)
  also checks the extracted upstream overnight surface against a compounded leg.
- `credit_completion_oracle.py` and `rates_completion_oracle.py`: QuantLib 1.43;
  CDS prices retain 1e-8 and fair quotes 1e-12; rate discounts retain 1e-12.
- `credit_jumps_oracle.cpp`: QuantLib 1.43; strict-boundary survival retains 1e-15.
- `isda_helpers.csv`: the [core generator](../crates/libitofin/tests/fixtures/isda_helpers/generator.py)
  produces the same 18 binding rows; survival and implied quotes retain 1e-10.
  Binding discount scenarios rebuild the market; Rust additionally checks live
  discount-quote recalibration. Density bootstrap fair quotes retain 1e-6.
- [Swaption matrix oracle](../crates/libitofin/tests/fixtures/swaption_matrix/README.md):
  Python/Go check 120 volatility nodes at `1e-16` and live quote/date observations.
  Rust additionally checks Black NPVs at `1e-12`, volatility recovery at `1e-6`,
  and quote-handle relinks. No public instrument implied-volatility API is added.
- `calibration_completion_oracle.py`: QuantLib 1.43; perturbed Heston smile
  distinguishes nonzero signed residuals. Parameters retain 3e-3 for Heston and
  1.3e-5 for Hull-White; signed residuals retain 1e-8.
- [Lazy inflation oracle](../crates/libitofin/tests/fixtures/lazy_inflation_base/README.md):
  75 QuantLib node rates at `1e-12` and five forecasts at `1e-7`; full node dates
  match fresh curves. Bindings rebuild the corrected-history scenario and exercise
  live fixing/date/quote updates; Rust also clears and repopulates history.
- `inflation_completion_oracle.cpp` and its reproduction guide: pinned independent
  QuantLib source; exact dates and 1e-12 node/time/rate comparisons.

Run `bash scripts/check_go_bindings.sh` for native tests, C/C++ clients, Go vet,
uncached race/cgocheck2 tests, and the portfolio example. Platform CI and release
acceptance are recorded separately in [the validation record](go-bindings-followups.md).

## Earlier triage provenance

At `4c48647e`, 166 declarations lacked explicit test references: 93 containing
types, 46 methods and 27 enum members. Inferred containing types did not need
artificial type-name tests. #1005 added Hull-White error/default oracles, CDS
bootstrap repricing, analytical hazard queries, inflation metadata and retained
relinking tests. #1003/#1004 subsequently completed the RNG/calendar surface.
The [historical validation record](go-bindings-followups.md) preserves those
revision-specific counts; missing references never implied unexecuted code.
