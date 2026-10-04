# Reviewed Rust priority additions

This batch reconciles the 28 High rows from the fork inventory with the current
parent implementation, QuantLib and the open issue board. Ten rows are excluded
because existing issues already own them. The remaining 18 rows combine new
capabilities with validation of already-delivered behavior, not 18 new modules.

- **Retained:** 18 rows: 1-10, 12, 13, 16, 22, 23, 26-28.
- **Excluded:** 10 rows: 11, 14-15, 17-21, 24-25.

## Availability

These additions are Rust-core APIs. They do not create new Python, C or Go
facades. Existing binding constructors, outputs and generated surfaces retain
their contracts. Generic numerical contracts intentionally remain Rust-only.
No release or complete QuantLib product-family parity is implied.

## Retained scope

| Area | Reviewed capabilities | Numerical and behavior contract |
| --- | --- | --- |
| Money and direct FX | `Money`, `ExchangeRate` | Checked same-currency arithmetic and reciprocal direct conversion reject invalid inputs and overflow. Use `checked_new` at untrusted boundaries; `new` retains the fork's unchecked value construction. No manager, automatic conversion or cross-rate chaining. |
| FX quotation | `BlackDeltaCalculator` | Spot, forward and premium-adjusted deltas, delta-to-strike inversion and ATM conventions. Premium-adjusted calls select QuantLib's higher-strike root. Put-call-50 ATM requires unadjusted forward delta. |
| Ibor coupons | Fixing/arrears controls and accessors | Existing constructors remain unchanged. Cached fixing value dates use the index fixing calendar. Holiday fixtures distinguish this from a joint settlement calendar. |
| Swap schedules | Independent leg rules and explicit stubs | Both legs support separate first and next-to-last dates, with schedule-compatible end-of-month normalization and fallible invalid-date checks. |
| Leg spreads | `CashFlows::npv_at_z_spread`, `z_spread` | Parallel zero-rate spreads preserve day count, compounding, settlement-flow inclusion and NPV-date normalization. Solver and curve errors propagate. |
| Bond quotation | Flat-yield price wrappers, existing curve prices and yield inversion | Clean/dirty prices are per 100 of outstanding settlement notional. Ex-coupon and amortization conventions are preserved. Divide-first normalization avoids intermediate overflow for representable large amounts. |
| Swaption quotation | `Swaption::implied_volatility` | European Black/Bachelier inversion supports Black displacement and spot/forward target prices. A temporary engine leaves the installed swaption engine unchanged. Unsupported exercise, impossible prices, malformed bounds and solver exhaustion return errors. |
| Numerical contracts | Forward-measure horizon and covariance conversion | The process horizon setter validates finite nonnegative values and notifies observers; direct `ForwardMeasureTime` state updates do not notify. Covariance is a deterministic conversion from standard deviations and correlation, not a sample estimator. It neither checks positive semidefiniteness nor repairs correlation inputs. |
| Compact linear algebra | `TridiagonalOperator` | Compact three-diagonal application, Thomas solves and reusable output buffers. Fallible constructors, application and solves reject invalid inputs and unusable pivots. Infallible endpoint setters and arithmetic operators can panic on invalid inputs or overflow. No sparse matrix export or pivoting solver is added. |
| Existing 1D FD queries | `Fdm1DimSolver`, `FdmBlackScholesSolver` | Existing value/Greek formulas and theta availability remain unchanged. Invalid coordinates are rejected, and observable process changes invalidate cached Black-Scholes snapshots. |
| Callable fixed-rate bonds | `CallableFixedRateBond`, `TreeCallableFixedRateBondEngine` | Hard issuer-call/holder-put decisions use an event-aligned Hull-White tree. Clean exercise prices include indenture accrued interest independently of market ex-coupon rules. Live curve/model updates rebuild valuation. |

## Checked money and FX

```rust
use libitofin::currency::Currency;
use libitofin::errors::QlResult;
use libitofin::exchangerate::ExchangeRate;
use libitofin::money::Money;

fn convert() -> QlResult<Money> {
    let amount = Money::checked_new(Currency::eur(), 100.0)?;
    let rate = ExchangeRate::checked_new(Currency::eur(), Currency::usd(), 1.08)?;
    rate.exchange(&amount)
}
```

A currency mismatch is an error, not an implicit FX trade. Negative monetary
amounts are valid; exchange rates must be finite and positive.

## Retained API limits

- Ibor arrears construction and fixing-date controls do not add convexity pricing.
  The existing `BlackIborCouponPricer` rejects in-arrears swaplet pricing, even
  when an optionlet volatility surface is supplied.
- FX delta inversion requires positive standard deviation and an admissible
  interior delta. Endpoint deltas and numerically unresolved inversions return
  errors. Raw Money scalar operators, unlike checked methods, remain unchecked.
- `TridiagonalOperator::solve_for_into` reuses the output allocation but allocates
  temporary elimination workspace. Numerical failure can leave partial output.
  Nonsingular systems requiring row pivoting can still be rejected.
- Black-Scholes solver cache refresh retains the fixed mesh and process object.
  Market-input handle relinks are supported, but process-object relinking and
  automatic mesh regeneration are not. Theta is unavailable when its snapshot
  would collide with a time-zero event.

## Callable-bond limits

- Fixed-rate bonds and hard calls/puts only, priced with Hull-White.
- Exercise rights are dated clean or dirty prices per 100, not percentage values
  passed as fractions.
- Settlement must not precede the curve reference date.
- Malformed schedules, expired exercise rights, invalid values and unavailable
  live curve handles are handled explicitly.
- No zero-coupon convenience class, Black callable engine, OAS, implied
  volatility or effective-duration API is introduced by this slice.
- Clearing a live Hull-White curve handle no longer panics in observer refresh.
  A callable price request fails while the handle is unavailable and recovers
  after a valid relink; no stale price is presented as current valuation.

## Existing-board exclusions

| High rows | Existing owner | Excluded work |
| --- | --- | --- |
| 11 | [#466](https://github.com/benbenbang/libitofin/issues/466) | Two-factor short-rate model contract |
| 14-15 | [#636](https://github.com/benbenbang/libitofin/issues/636) | Concrete and multidimensional boundary conditions |
| 17-21 | [#636](https://github.com/benbenbang/libitofin/issues/636) | Mixed operators, ADI schemes, expanded rollback and 2D solver |
| 24-25 | [#636](https://github.com/benbenbang/libitofin/issues/636) | Heston and Hull-White finite-difference engines |

These tickets are neither closed nor activated by this batch. Temporary excluded
drafts are not production code and do not establish delivered coverage.

Two superficially similar existing tickets do not own the retained capabilities:
[#1166](https://github.com/benbenbang/libitofin/issues/1166) estimates weighted
sample covariance, rather than converting standard deviations and correlation;
[#350](https://github.com/benbenbang/libitofin/issues/350) tracks cap/floor implied
volatility, rather than swaption implied volatility.

## Independent evidence

The [fixture provenance and replay instructions](https://github.com/benbenbang/libitofin/tree/main/crates/libitofin/tests/fixtures/high28)
identify compiled `QuantLib==1.43` independently from inspected source revision
`9863b578af0caa4cecabf697196533e84a8308b6` (`1.43-dev`). No same-revision build
claim is made. Rust tests bake the independently generated values and require no
installed QuantLib.

New tests cover four FX delta conventions, five spread compoundings, bond
settlement/ex-coupon/amortization normalization, holiday Ibor fixings, mixed
swap-leg stub rules, spot/forward swaption inversions, callable event ordering,
large-notional quotes and live-handle failure/recovery. Independent tridiagonal
QA checks dense-reference residuals and allocation reuse. Existing tolerances
are not relaxed.
