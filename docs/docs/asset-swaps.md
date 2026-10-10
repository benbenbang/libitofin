# Par asset swaps

`libitofin::instruments::AssetSwap` exchanges a bullet bond leg for an Ibor leg.
This additive Rust-core API uses the existing `DiscountingSwapEngine`.

## Construction

```rust,ignore
let mut swap = AssetSwap::new(
    true,
    bond,
    102.5,
    euribor,
    0.001,
    None,
    None,
)?;
swap.base_mut().set_pricing_engine(engine);
let npv = swap.npv()?;
let spread = swap.fair_spread()?;
let price = swap.fair_clean_price()?;
```

- Bond and index must share their `Settings` handle.
- An omitted schedule runs backwards from bond settlement to full maturity,
  using index tenor/calendar/convention, without end-of-month preservation.
- Floating payments use Following and gearing one.
- Dirty upfront is `(clean price + accrued per 100 - 100) * notional / 100`.
  It belongs to the floating leg, followed by Ibor coupons and final notional.
- Bond coupons already occurred on upfront are removed. Today's-flow settings
  can override normal same-day exclusion, consistently with native QuantLib.
- The actual bond redemption is preserved, even when its amount is not par.
- Paying the bond leg gives negative bond NPV and positive floating BPS;
  reversing sides reverses signed values without changing fair spread or price.

## Pricing and limits

`fair_spread = spread - NPV / floating_leg_bps * 1e-4`.
Fair clean price follows the native formula through the discounted upfront,
in per-100 units. It zeroes NPV only when the upfront payment is included;
if settlement rules exclude upfront, the returned formula need not reprice NPV.
Curve quotes, forwarding/discount handles, engine replacement and evaluation-date
changes invalidate cached results through existing observers. Expired NPVs are
zero; fair values with zero BPS/discount are errors. Seasoned deals whose upfront
predates the discount curve reference date have no fair-clean-price result.

Market/non-par swaps, amortizing bonds, custom final repayments, partial-maturity
swaps, day-tenor/overnight indexes and specialized asset-swap engines are not
supported. No C, Go or Python facade is added. Coupon forecast mode follows the
existing Settings contract; validation fixtures explicitly use at-par Ibor coupons.

Native QuantLib 1.43 fixtures cover dirty accrual, same-day cutoff/settings,
non-par bond redemption, forward starts, generated schedules, separate curve
bumps, payer signs and fair values. The independently inspected source is pinned
at `9863b578af0caa4cecabf697196533e84a8308b6`, a different revision from the wheel.
