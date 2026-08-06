"""Build and price a vanilla fixed-vs-floating interest-rate swap.

Two ways to get the same swap:

* `MakeVanillaSwap` - a builder that fills in the market conventions (fixed-leg
  tenor and day count, floating leg from the index) from a tenor and an index.
* `VanillaSwap` - the explicit form, where every schedule and day count is
  spelled out by hand.

The swap is priced by a `DiscountingSwapEngine`, which here is attached with
`swap.set_engine(curve, settings)` (the engine discounts both legs off the given
curve). The fixture is QuantLib's Jamshidian 5Y swap; the fair rate and NPV
below reproduce the pins in `test_vanilla_swap.py`, so they double as a
self-check.

Run it with:

    python example/python/vanilla_swap.py
"""

# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.instruments import MakeVanillaSwap, SwapType, VanillaSwap
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

REF = Date(15, 1, 2026)
START = Date(15, 1, 2028)
END = Date(15, 1, 2033)


def make_market():
    settings = Settings()
    settings.set_evaluation_date(REF)
    # A flat 3% continuous curve for both forwarding and discounting.
    curve = FlatForward(REF, 0.03, DayCounter.actual365_fixed())
    index = Euribor.six_months(curve, settings)
    return settings, curve, index


def priced_with_builder():
    """A par 5Y payer swap via the builder: `fixed_rate=None` fills the fixed
    leg with the fair rate, so the swap prices to zero."""
    settings, _curve, index = make_market()
    swap = MakeVanillaSwap(
        Period(5, "Years"),
        index,
        settings,
        fixed_rate=None,
        effective_date=START,
        nominal=100.0,
    ).build()  # MakeVanillaSwap is a builder: .build() returns the swap.
    return swap


def priced_by_hand():
    """The same swap spelled out: annual fixed leg on 30/360 bond basis,
    semiannual floating leg on Euribor6M / Actual360, priced at a 3% coupon."""
    settings, curve, index = make_market()
    fixed = Schedule(
        START,
        END,
        Frequency.Annual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    floating = Schedule(
        START,
        END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    swap = VanillaSwap(
        SwapType.Payer,
        100.0,  # nominal
        fixed,
        0.03,  # fixed rate
        DayCounter.thirty360_bond_basis(),
        floating,
        index,
        0.0,  # floating spread
        DayCounter.actual360(),
        settings,
    )
    # DiscountingSwapEngine off `curve`: VanillaSwap.set_engine takes the curve
    # and settings directly, not a separate engine object.
    swap.set_engine(curve, settings)
    return swap


def main() -> None:
    par = priced_with_builder()
    print("MakeVanillaSwap, par 5Y payer (fixed_rate=None):")
    print(f"  fair_rate = {par.fair_rate() * 100:.6f}%")
    print(f"  fixed_rate(filled) = {par.fixed_rate() * 100:.6f}%")
    print(f"  NPV       = {par.npv():.10f}  (par swap -> ~0)")

    hand = priced_by_hand()
    print("\nHand-built VanillaSwap, 5Y payer at a 3% coupon:")
    print(f"  nominal   = {hand.nominal():.2f}")
    print(f"  fixed_rate= {hand.fixed_rate() * 100:.4f}%")
    print(f"  fair_rate = {hand.fair_rate() * 100:.6f}%")
    print(f"  NPV       = {hand.npv():.10f}")


if __name__ == "__main__":
    main()
