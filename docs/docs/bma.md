# Municipal BMA swaps

`BMAIndex`, `AverageBMACoupon`, `BMASwap` and `BMASwapRateHelper` are available
in Rust, Python and Go. They model the weekly SIFMA municipal rate, its
calendar-day average, and swaps exchanging that average for a fraction of Ibor.

## Build and price

1. Create both indexes with the same settings. Supply a forecast curve to the
   Ibor index; a BMA index may start without a curve when used by a helper.
2. Add the required historical BMA fixings. Fixings fall on Wednesday or the
   first subsequent US Government Bond business day. Missing history is an
   error; it is never replaced silently by a forecast.
3. Build the Ibor and BMA schedules with their respective calendars and payment
   conventions, then construct `BMASwap`. **Payer pays BMA and receives Ibor.**
4. Attach a discount curve. Python uses `swap.set_engine(curve, settings)`;
   Go uses `swap.SetEngine(curve, settings)`. Rust uses `DiscountingSwapEngine`
   through the standard instrument interface.
5. Read NPV, fair Ibor fraction/spread and signed leg NPV/BPS. Leg zero is Ibor;
   leg one is BMA. Results retain their market inputs and invalidate on updates.

## Fit a municipal curve

Construct `BMASwapRateHelper` with a live fraction quote, maturity tenor,
settlement/calendar conventions, BMA payment period/day counter, and both
indexes. Feed the helpers into an existing piecewise yield-curve constructor.
The fitted curve forecasts BMA; the supplied Ibor curve forecasts and discounts
the helper's swap. Fraction quotes use decimal units: 67.56% means `0.6756`.

The helper extends its pillar to the next BMA value date after swap maturity,
so the final weekly fixing can be forecast. It uses a weak, unobserved link to
the fitted curve and explicitly refreshes pricing during bootstrap. Quote,
fixing and evaluation-date changes invalidate the curve; invalid dates or
missing history propagate errors and recover when corrected.

## Coupon and history access

`AverageBMACoupon` exposes rate, payment amount, accrual period and copied fixing
dates. Its arithmetic average weights each weekly fixing by the actual number
of accrual days it covers, then applies gearing and spread. Reference dates
support irregular accrual periods. A preceding-adjusted payment may occur before
the unadjusted accrual end.

BMA indexes expose fixing-calendar, valid-fixing, value-date, maturity-date and
bracketing fixing-schedule queries, plus historical-fixing reads, writes and
clearing. Python and Go retain native dependencies when the originating wrapper
is released; Go handles still follow the owning session's lifetime.

The [independent oracle](https://github.com/benbenbang/libitofin/tree/main/crates/libitofin/tests/fixtures/bma)
contains QuantLib 1.43 prices for ten maturities from 1Y through 30Y. Iterative
bootstrap reprices the original fractions within `1e-9`; Rust's local-bootstrap
acceptance retains QuantLib's `1e-6` tolerance. These APIs cover plain municipal
swaps and averaged coupons; they do not add a municipal option model.
