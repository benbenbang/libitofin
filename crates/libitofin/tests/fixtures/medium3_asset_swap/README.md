# Par asset swap native fixtures

- Oracle: compiled PyPI `QuantLib==1.43`, with at-par Ibor coupons explicitly selected.
- Source inspected independently: original QuantLib checkout commit
  `9863b578af0caa4cecabf697196533e84a8308b6`, especially
  `ql/instruments/assetswap.cpp` and `.hpp`.
- The wheel is a different revision from that pinned checkout. These are wheel
  fixtures, not a claim of executing the pinned C++ build.
- Regenerate: `python generate.py` using an environment with QuantLib 1.43.
  The tests read frozen CSV and never import Python or require QuantLib in CI.
- `prices.csv` records native NPV, signed leg NPVs, floating BPS, fair spread,
  fair clean price, dirty upfront, accrual and leg counts.
- `flows.csv` records native dates and amounts for default settlement-generated
  schedules, coupon-date cutoff and a 105% bond redemption. The swap remains
  par: the floating notional and final exchange remain 1000.
- Dates are QuantLib/Excel serial numbers. Rates/spreads use decimals, prices
  are per 100 and monetary values are in the 1000-face bond's units.
- The explicit December 30, 2024 fixing is 0.031. It is used only by swaps
  starting January 2, 2025. Other coupons forecast off independent flat curves.
- Default settings leave today's-flow inclusion unset. The dedicated override
  case sets it true. This deliberately tests the native `hasOccurred` override:
  a coupon on upfront is normally excluded, but today's-flow settings can retain
  it, and the bond's accrued amount follows the same setting.
