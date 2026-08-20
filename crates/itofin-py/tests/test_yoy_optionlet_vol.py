"""The live-quote year-on-year optionlet surface (#860): the same two-year cap
the flat surface prices, quoted off a SimpleQuote the test then moves.

The fixture is the one the cap/floor binding runs on, reproducing the Rust
engine oracle `inflationcapfloorengines.rs:818`. The suite does not share
fixtures across test files, so it is transcribed here, and the four things about
it that are load-bearing are:

1. The RPI history is thirty-one published figures followed by TWO -999.0
   sentinels. Neither is ever read as a rate, but they carry the index's last
   fixing date - and with it the curve's base date - to 1 August 2007, which is
   the origin every number here is measured from.
2. The schedule those figures are filed on is generated BACKWARD, the core
   MakeSchedule default the Rust fixture leaves in place. The Python facade
   defaults to Forward, so the rule is passed explicitly; generating forward
   shifts every figure by a month.
3. The observation lag splits two ways: the leg and the surface observe ZERO
   days, only the fifteen bootstrap helpers observe two months. Pricing the leg
   at two months misses the cached value by about 47.
4. The nominal curve is Actual/Actual (ISDA) and the curve frequency Monthly
   (what UK RPI publishes), while the cap/floor schedule is Annual.

What this file adds over the flat-surface oracle next door is the second test.
A binding that read the quote's value once at construction would reproduce the
cached value exactly as well as a live one does, so only re-pricing the same
instrument after the quote moves tells the two apart.
"""

# itofin library
from itofin import Settings
from itofin.indexes import CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex
from itofin.instruments import CapFloorType, MakeYoYInflationCapFloor, YoYInflationCapFloor
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
CACHED_CAP = 219.452

FIX_DATA = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1,
    193.3, 193.6, 194.1, 193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5,
    199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2,
    207.3, -999.0, -999.0,
]

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


def _day_counter() -> DayCounter:
    return DayCounter.thirty360_bond_basis()


def _rpi_schedule() -> Schedule:
    """Monthly from 1 January 2005 to 13 August 2007, generated BACKWARD so the
    short front stub lands where the Rust fixture puts it."""
    return Schedule(
        Date(1, 1, 2005),
        Date(13, 8, 2007),
        Frequency.Monthly,
        UK,
        BusinessDayConvention.ModifiedFollowing,
        DateGeneration.Backward,
    )


def _market() -> tuple[Settings, YoYInflationIndex, FlatForward]:
    """The bootstrapped market: one Settings, the ratio year-on-year index over a
    UK RPI carrying the sentinel history, and the 5 % nominal curve. The index is
    left linked to the bootstrapped curve."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)

    rpi = ZeroInflationIndex.uk_rpi(settings)
    for date, figure in zip(_rpi_schedule().dates(), FIX_DATA):
        rpi.add_fixing(date, figure)

    index = YoYInflationIndex.from_underlying(rpi)
    nominal = FlatForward(TODAY, NOMINAL_RATE, DayCounter.actual_actual_isda())

    helpers = [
        YearOnYearInflationSwapHelper(
            SimpleQuote(rate / 100.0),
            HELPER_LAG,
            maturity,
            UK,
            BusinessDayConvention.ModifiedFollowing,
            _day_counter(),
            index,
            CpiInterpolationType.Flat,
            nominal,
            settings,
        )
        for maturity, rate in YY_DATA
    ]
    curve = PiecewiseYoYInflationCurve(
        TODAY,
        rpi.last_fixing_date(),
        YY_DATA[0][1] / 100.0,
        Frequency.Monthly,
        _day_counter(),
        helpers,
    )
    index.link_to(curve)
    return settings, index, nominal


def _quoted_surface(
    quote: SimpleQuote, settings: Settings
) -> ConstantYoYOptionletVolatility:
    """The live-quote surface, on the config the cached values were produced on:
    zero observation lag, like the leg's and unlike the helpers'."""
    return ConstantYoYOptionletVolatility.with_quote(
        quote,
        0,
        UK,
        BusinessDayConvention.ModifiedFollowing,
        _day_counter(),
        NO_LAG,
        Frequency.Annual,
        False,
        -1.0,
        100.0,
        settings,
    )


def _flat_surface(settings: Settings) -> ConstantYoYOptionletVolatility:
    """The same surface through the value-taking constructor, for the equality
    twin below."""
    return ConstantYoYOptionletVolatility(
        VOLATILITY,
        0,
        UK,
        BusinessDayConvention.ModifiedFollowing,
        _day_counter(),
        NO_LAG,
        Frequency.Annual,
        False,
        -1.0,
        100.0,
        settings,
    )


def _cap(settings: Settings, index: YoYInflationIndex) -> YoYInflationCapFloor:
    """A two-year factory-built cap struck at 2.95 %. The builder's own defaults -
    annual schedule, ModifiedFollowing payment roll, Thirty360 bond basis, no
    fixing days - are what the Rust fixture's hand-built leg uses."""
    return MakeYoYInflationCapFloor(
        CapFloorType.Cap,
        index,
        2,
        UK,
        NO_LAG,
        CpiInterpolationType.Flat,
        settings,
        nominal=NOMINAL,
        strike=STRIKE,
    ).build()


def _priced(
    settings: Settings,
    index: YoYInflationIndex,
    nominal: FlatForward,
    surface: ConstantYoYOptionletVolatility,
) -> float:
    """One instrument, one engine: an engine carries the arguments and results of
    the contract it last priced, so two instruments never share one."""
    instrument = _cap(settings, index)
    instrument.set_engine(YoYInflationCapFloorEngine.black(index, surface, nominal))
    return instrument.npv()


def test_a_quoted_surface_reaches_the_cached_black_value():
    """The wiring oracle: a quote at 1 % has to price the cap at the level the
    flat 1 % surface does, which is QuantLib's cached 219.452 within its own 0.02
    on a 1e6 notional.

    That tolerance is 1e-4 relative and measured against a literal another build
    produced, so a level slip can sit inside it: a quote 0.01 % off prices at
    219.4672, which the band still admits. The equality is what refuses it. Both
    routes hand the engine a flat surface at the same level and run the same
    float sequence over the same curve, so they agree bit for bit.

    Equality is the sharper check for the level, and the level is what this
    constructor moves; it is not a general config guard, because most of the
    surface's other arguments cannot bite on this fixture. The variance the
    engine reads is the volatility squared times the span from the base date to
    the observed fixing date, and both dates are quantized by the same
    frequency, so they shift in lockstep: a Monthly surface here prices exactly
    as an Annual one, and a two-month observation lag under Annual snaps back to
    the same base date. Only the two together move a number."""
    settings, index, nominal = _market()

    quoted = _priced(settings, index, nominal, _quoted_surface(SimpleQuote(VOLATILITY), settings))
    flat = _priced(settings, index, nominal, _flat_surface(settings))

    print(f"quoted cap = {quoted:.4f} (cached {CACHED_CAP}), flat cap = {flat:.4f}")
    assert abs(quoted - CACHED_CAP) < 0.02
    assert quoted == flat


def test_moving_the_quote_reprices_the_cap():
    """What the value-taking constructor cannot express, and this ticket's reason
    to exist. The quote is retained by the surface, the surface observes it, the
    engine observes the surface and the instrument observes the engine, so
    doubling the volatility reprices a cap already built - nothing here is
    rebuilt after the set_value. A binding that read the quote's f64 once at
    construction, or one that dropped the notification, would hand back the same
    number twice.

    The starting value is pinned to the cached one as well, so the increase
    cannot be read off a fixture that was broken to begin with."""
    settings, index, nominal = _market()

    quote = SimpleQuote(VOLATILITY)
    surface = _quoted_surface(quote, settings)
    instrument = _cap(settings, index)
    instrument.set_engine(YoYInflationCapFloorEngine.black(index, surface, nominal))

    before = instrument.npv()
    quote.set_value(2.0 * VOLATILITY)
    after = instrument.npv()

    print(f"cap at 1 % = {before:.4f} (cached {CACHED_CAP}), at 2 % = {after:.4f}")
    assert abs(before - CACHED_CAP) < 0.02
    assert after > before
