# Go behavioral test evidence

[#1036](https://github.com/benbenbang/libitofin/issues/1036) completes the bounded
cases deferred by the original #1005 triage. Tests below use independent
QuantLib or analytical expectations; API mappings alone do not prove behavior.
This inventory does not claim exhaustive QuantLib product coverage.

## Completed cases

| Scope | Executable evidence |
| --- | --- |
| Zero/YoY index representations and retained ratio identity | `TestInflationCompletionIndexRepresentations` |
| Piecewise zero/YoY dates, times, nodes and detached copies | `TestInflationCompletionPiecewiseDetachedOutputs` |
| Zero/YoY helper latest and pillar dates, flat/linear interpolation | `TestInflationCompletionHelperDates` |
| CDS rebate settlement, weekend handling and absent rebate | `TestCreditCompletionRebateSettlementBusinessDays` |
| CDS builder, explicit schedule, premium/default cashflows, cached calculation and fair-upfront zero NPV | `TestCreditCompletionBuilderCashflowsAndFairUpfront` |
| Midpoint implied hazard, ISDA dispatch and buyer/seller signs | `TestCreditCompletionMidpointHazardAndProtectionSigns` |
| Hazard dates/rates detached copies and non-flat ISDA NoFix/Taylor, Flat/Piecewise combinations | `TestCreditCompletionHazardArraysAndNonflatISDA` |
| Swaption cash/physical settlement, both annuity models, unsupported combinations, payer/receiver fair rates | `TestRatesCompletionSwaptionSettlementAndReceiver` |
| Dirty/clean bond quotes with nonzero accrued interest | `TestRatesCompletionDirtyBondWithAccruedInterest` |
| Valid Custom/ASX futures dates, prices and analytical forward/discount factors | `TestRatesCompletionCustomAndASXFutures` |
| Compound OIS numerical oracle, safe rejection of unsupported Simple, same-session recovery | `TestRatesCompletionOISAveragingOracle` |
| Heston PriceError/ImpliedVolError fitted parameters and signed residuals | `TestHestonCalibrationPriceAndImpliedVolOracles` |
| Hull-White fixed reversion and mixed omitted/explicit optimizer and stopping settings | `TestHullWhiteCalibrationFixedReversionAndOptionalCombinations` |

The CDS builder includes the final accrual day; the current explicit Go
constructor does not expose that convention. Each is checked against its own
matching QuantLib contract, and the one-day premium difference is reconstructed
analytically. Their NPVs are intentionally different.

Simple-averaged overnight coupons remain unimplemented in the Rust core:
[#1038](https://github.com/benbenbang/libitofin/issues/1038). The C boundary rejects
this option before construction with an invalid-input error, leaving the session
usable. The Simple oracle is retained for that future implementation; successful
Simple pricing is not claimed.

## Oracle reproduction and tolerances

Sources and fixtures live in [`sdk/go/testdata`](../sdk/go/testdata/):

- `credit_completion_oracle.py` and `rates_completion_oracle.py`: QuantLib 1.43;
  CDS prices retain 1e-8 and fair quotes 1e-12; rate discounts retain 1e-12.
- `calibration_completion_oracle.py`: QuantLib 1.43; perturbed Heston smile
  distinguishes nonzero signed residuals. Parameters retain 3e-3 for Heston and
  1.3e-5 for Hull-White; signed residuals retain 1e-8.
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
