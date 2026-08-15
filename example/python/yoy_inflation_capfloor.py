"""Price a year-on-year inflation cap and floor under three distributions.

A `YoYInflationCapFloor` is a strip of optionlets on the year-on-year rate a
`YoYInflationIndex` publishes, one per coupon of an annual inflation leg. It is
built through `MakeYoYInflationCapFloor`, the standard market builder: the core
builder is a consumed-self fluent chain that does not cross the FFI boundary, so
the Python facade takes the whole configuration up front and assembles the chain
inside `build()`.

Three things this shows that no other example does.

* The distribution is chosen by the *constructor*, mirroring C++'s three engine
  classes rather than being passed as an argument: `YoYInflationCapFloorEngine`
  has `black` (lognormal), `unit_displaced` (lognormal in 1 + rate) and
  `bachelier` (normal). On this fixture the last two sit two orders of magnitude
  above Black, so a constructor wired to the wrong distribution cannot hide.
* An engine carries the arguments and results of whatever it last priced, so a
  cap and a floor priced together want one engine each.
* `strike` and `atm_strike` are mutually exclusive and exactly one is required.
  The refusal lands in `build()`, not at the setters, because a fluent setter
  returning `Self` has nowhere to raise from.

The numbers reproduce QuantLib's `inflationcapfloorengines.cpp:452-522`
(`testCachedValue`), as transcribed in
`crates/itofin-py/tests/test_yoy_inflation_capfloor.py`. QuantLib's own
tolerance is 0.02 on a notional of a million, which these only reach if the
curve, the base date, the zero observation lag and the distribution are all
right at once.

Fixture warning. `yoy_inflation_swap.py` next door runs a *different* UK RPI
fixture, and the two must not be crossed. This one files thirty-one published
figures followed by TWO -999.0 sentinels on a BACKWARD-generated schedule, which
carries the index's last fixing date - and with it the curve's base date - out
to 1 August 2007. The leg, the volatility surface and the builder observe ZERO
days of lag while only the fifteen bootstrap helpers observe two months (in C++
this comes of a `CommonVars` member being shadowed by a local in the constructor
body); pricing the leg at two months instead misses every cached value by about
47. The nominal curve is Act/Act (ISDA), not Act/360.

Run it with:

    python example/python/yoy_inflation_capfloor.py
"""

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex
from itofin.instruments import CapFloorType, MakeYoYInflationCapFloor
from itofin.pricingengines import YoYInflationCapFloorEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    ConstantYoYOptionletVolatility,
    FlatForward,
    PiecewiseYoYInflationCurve,
    YearOnYearInflationSwapHelper,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

UK = Calendar.united_kingdom()
TODAY = UK.adjust(Date(13, 8, 2007), BusinessDayConvention.Following)
NO_LAG = Period(0, "Days")
HELPER_LAG = Period(2, "Months")
NOMINAL = 1_000_000.0
NOMINAL_RATE = 0.05
STRIKE = 0.0295
VOLATILITY = 0.01
DAY_COUNTER = DayCounter.thirty360_bond_basis()

# Thirty-one published UK RPI figures from January 2005, then two -999.0
# sentinels. Neither sentinel is ever read as a rate - every fixing date here is
# 2008 or later, so the ratio index forecasts off the curve - but they carry the
# curve's base date from June to 1 August 2007, the origin every number below is
# measured from. Dropping them silently moves every optionlet.
FIX_DATA = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1,
    193.3, 193.6, 194.1, 193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5,
    199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2,
    207.3, -999.0, -999.0,
]

# (maturity, quoted year-on-year swap rate in percent).
YY_DATA = [
    (Date(13, 8, 2008), 2.95),
    (Date(13, 8, 2009), 2.95),
    (Date(13, 8, 2010), 2.93),
    (Date(15, 8, 2011), 2.955),
    (Date(13, 8, 2012), 2.945),
    (Date(13, 8, 2013), 2.985),
    (Date(13, 8, 2014), 3.01),
    (Date(13, 8, 2015), 3.035),
    (Date(13, 8, 2016), 3.055),
    (Date(13, 8, 2017), 3.075),
    (Date(13, 8, 2019), 3.105),
    (Date(15, 8, 2022), 3.135),
    (Date(13, 8, 2027), 3.155),
    (Date(13, 8, 2032), 3.145),
    (Date(13, 8, 2037), 3.145),
]

# inflationcapfloorengines.cpp:452-522, by distribution: (cap, floor, tolerance).
CACHED = {
    "black": (219.452, 314.641, 0.02),
    "unit_displaced": (9114.61, 9209.8, 0.22),
    "bachelier": (8852.4, 8947.59, 0.22),
}


def rpi_schedule() -> Schedule:
    """The dates the RPI figures are filed on: monthly from 1 January 2005 to
    13 August 2007, generated BACKWARD so the schedule walks back from the 13th
    and carries a short front stub. The Python `Schedule` facade defaults to
    Forward, so the rule is passed explicitly; generating forward instead would
    shift every figure by a month."""
    return Schedule(
        Date(1, 1, 2005),
        Date(13, 8, 2007),
        Frequency.Monthly,
        UK,
        BusinessDayConvention.ModifiedFollowing,
        DateGeneration.Backward,
    )


def build_market():
    """One Settings, a ratio year-on-year index over a UK RPI carrying the
    sentinel history, and the 5% nominal curve. The index is returned linked to
    the bootstrapped year-on-year curve."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)

    rpi = ZeroInflationIndex.uk_rpi(settings)
    for date, figure in zip(rpi_schedule().dates(), FIX_DATA):
        rpi.add_fixing(date, figure)

    # A ratio index derives its year-on-year rate from two RPI fixings a year
    # apart; it owns no history of its own, so the fixings go on the underlying.
    index = YoYInflationIndex.from_underlying(rpi)
    nominal = FlatForward(TODAY, NOMINAL_RATE, DayCounter.actual_actual_isda())

    helpers = [
        YearOnYearInflationSwapHelper(
            SimpleQuote(rate / 100.0),
            HELPER_LAG,  # only the helpers observe two months
            maturity,
            UK,
            BusinessDayConvention.ModifiedFollowing,
            DAY_COUNTER,
            index,
            CpiInterpolationType.Flat,
            nominal,
            settings,
        )
        for maturity, rate in YY_DATA
    ]
    curve = PiecewiseYoYInflationCurve(
        TODAY,
        rpi.last_fixing_date(),  # the base date the sentinels carried to August
        YY_DATA[0][1] / 100.0,  # the base year-on-year rate at node zero
        Frequency.Monthly,  # what UK RPI publishes, not the annual leg's
        DAY_COUNTER,
        helpers,
    )
    index.link_to(curve)
    return settings, index, nominal


def surface(settings: Settings) -> ConstantYoYOptionletVolatility:
    """The flat 1% surface the cached values were produced on. Its observation
    lag is zero, like the leg's and unlike the helpers'."""
    return ConstantYoYOptionletVolatility(
        VOLATILITY,
        0,  # settlement days
        UK,
        BusinessDayConvention.ModifiedFollowing,
        DAY_COUNTER,
        NO_LAG,
        Frequency.Annual,
        False,  # index is interpolated
        -1.0,  # min strike
        100.0,  # max strike
        settings,
    )


def cap_floor(settings, index, cap_floor_type, **kwargs):
    """A two-year factory-built cap or floor. The builder's own defaults - an
    annual schedule, a ModifiedFollowing payment roll, 30/360 bond basis, no
    fixing days, every optionlet kept - are exactly the cached fixture's, so
    nothing beyond the strike and the nominal has to be said."""
    return MakeYoYInflationCapFloor(
        cap_floor_type,
        index,
        2,  # length in years
        UK,
        NO_LAG,
        CpiInterpolationType.Flat,
        settings,
        nominal=NOMINAL,
        **kwargs,
    ).build()


def priced(settings, index, nominal, cap_floor_type, distribution) -> float:
    """One instrument, one engine: an engine carries the results of whatever it
    last priced, so a cap and a floor never share one."""
    instrument = cap_floor(settings, index, cap_floor_type, strike=STRIKE)
    engine = getattr(YoYInflationCapFloorEngine, distribution)(index, surface(settings), nominal)
    instrument.set_engine(engine)
    return instrument.npv()


def price_under_each_distribution(settings, index, nominal) -> None:
    print(f"Two-year year-on-year cap and floor struck at {STRIKE:.2%}, notional {NOMINAL:,.0f}, vol {VOLATILITY:.0%}")
    for distribution, (cached_cap, cached_floor, tolerance) in CACHED.items():
        cap = priced(settings, index, nominal, CapFloorType.Cap, distribution)
        floor = priced(settings, index, nominal, CapFloorType.Floor, distribution)
        print(f"\n  {distribution}:")
        print(
            f"    cap   = {cap:10.4f}   cached={cached_cap:>9}   "
            f"|diff|={abs(cap - cached_cap):.4f}  (tol {tolerance})"
        )
        print(
            f"    floor = {floor:10.4f}   cached={cached_floor:>9}   "
            f"|diff|={abs(floor - cached_floor):.4f}  (tol {tolerance})"
        )


def fill_the_strike_at_the_money(settings, index, nominal) -> None:
    """With no strike given, `atm_strike` resolves one off the nominal curve.
    The rate that lands on the leg is the rate the instrument itself would
    reprice at, which `atm_rate` reads back independently.

    Trimming happens before the at-the-money fill, so `as_optionlet` and
    `first_caplet_excluded` would change what an unset strike resolves to: the
    rate repricing whatever survives, not the whole leg's."""
    cap = cap_floor(settings, index, CapFloorType.Cap, atm_strike=nominal)
    struck = cap.cap_rates()[0]

    print("\n\nAt-the-money fill (no strike given, resolved off the nominal curve):")
    print(f"  strike on the leg = {struck:.10f}")
    print(f"  atm_rate()        = {cap.atm_rate(nominal):.10f}   |diff|={abs(struck - cap.atm_rate(nominal)):.2e}")
    print(f"  optionlets        = {cap.coupon_count()}   floor rates on a cap = {cap.floor_rates()}")
    print(f"  runs {cap.start_date()} -> {cap.maturity_date()}")


def strike_and_atm_are_exclusive(settings, index, nominal) -> None:
    """Both together and neither at all are refused, and both refusals surface
    from `build()`."""
    print("\n\nThe strike is required, and exactly one way of giving it:")
    for label, kwargs in [
        ("both a strike and an ATM curve", {"strike": STRIKE, "atm_strike": nominal}),
        ("neither", {}),
    ]:
        try:
            cap_floor(settings, index, CapFloorType.Cap, **kwargs)
        except ItofinError as error:
            print(f"  {label:32} -> {type(error).__name__}: {error}")


def main() -> None:
    settings, index, nominal = build_market()
    print(f"UK RPI last fixing date (the curve's base) = {index.underlying_index().last_fixing_date()}\n")

    price_under_each_distribution(settings, index, nominal)
    fill_the_strike_at_the_money(settings, index, nominal)
    strike_and_atm_are_exclusive(settings, index, nominal)


if __name__ == "__main__":
    main()
