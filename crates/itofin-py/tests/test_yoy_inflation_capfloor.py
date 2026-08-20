"""The year-on-year inflation cap/floor binding (#858): a two-year cap and floor
struck at 2.95 %, priced through the Python surface against the values QuantLib
caches.

This reproduces the Rust engine oracle
`inflationcapfloorengines.rs:818` (`testCachedValue`, `.cpp:452-522`), not the
year-on-year reprice milestone next door: the two fixtures differ in exactly the
spots that move these numbers. The six cached values are the only assertions in
that suite that would notice the fixture being subtly wrong - parity and
optionlet-sum consistency are internal identities that hold whatever the curve
says - so they are what a binding has to reproduce to have proved anything.

Fixture, and the four things about it that are load-bearing.

1. The RPI history is thirty-one published figures followed by TWO -999.0
   sentinels. Neither sentinel is ever read as a rate (every fixing date here is
   2008 or later, so the ratio index forecasts off the curve), but they move the
   index's last fixing date, and with it the curve's base date, to 1 August 2007
   - the origin every number below is measured from.
2. The schedule those figures are filed on runs monthly from 1 January 2005 to
   13 August 2007 under the default BACKWARD generation, so it walks back from
   the 13th and carries a short front stub: dates()[0] is 4 January 2005 and
   dates()[1] 13 January 2005, both in the January period. Filing quantizes to
   the publication period, so the store is thirty-one months from January 2005
   with the sentinels in July and August 2007. Generating forward instead would
   shift every figure by a month.
3. The observation lag splits two ways. The leg, the volatility surface and the
   builder all observe ZERO days; only the fifteen bootstrap helpers observe two
   months. In C++ this comes of a `CommonVars` member being shadowed by a local
   in the constructor body; pricing the leg at two months instead misses every
   cached value by about 47.
4. The nominal curve is Actual/Actual (ISDA), not Actual/360, and the curve
   frequency is Monthly (what UK RPI publishes, and what quantizes the curve's
   nodes) while the cap/floor schedule is Annual. Conflating the two frequencies
   moves every rate.

The volatility is 1 %, which is what the cached-value case passes; the 15 % in
the Rust module's `VOLS` array belongs to the parity sweep.

The raw constructors (#859) are exercised at the end of this file, off the same
fixture. They are what the Rust oracle itself calls
(`inflationcapfloorengines.rs:606-608`, over the leg at `:528`), so putting them
against the same two cached values is a like-for-like reproduction of it: the
factory path above reaches those numbers through a builder that assembles its own
leg, this one through a leg Python assembles and hands over coupon by coupon.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.cashflows import YoYInflationCoupon, YoYInflationLeg
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
CURVE_BASE = Date(1, 8, 2007)
NO_LAG = Period(0, "Days")
HELPER_LAG = Period(2, "Months")
NOMINAL = 1_000_000.0
NOMINAL_RATE = 0.05
STRIKE = 0.0295
VOLATILITY = 0.01

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
    """The dates the RPI figures are filed on: monthly from 1 January 2005 to
    13 August 2007, generated BACKWARD, which is the core MakeSchedule default
    the Rust fixture leaves in place. The Python facade defaults to Forward, so
    the rule is passed explicitly."""
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


def _surface(settings: Settings) -> ConstantYoYOptionletVolatility:
    """The flat 1 % surface the cached values were produced on. Its observation
    lag is zero, like the leg's and unlike the helpers'."""
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


def _cap_floor(
    settings: Settings,
    index: YoYInflationIndex,
    cap_floor_type: CapFloorType,
    **kwargs,
) -> YoYInflationCapFloor:
    """A two-year factory-built cap or floor. The builder's own defaults - annual
    schedule, ModifiedFollowing payment roll, Thirty360 bond basis, no fixing
    days - are exactly what the Rust fixture's hand-built leg uses, so nothing
    beyond the strike and the nominal has to be said."""
    return MakeYoYInflationCapFloor(
        cap_floor_type,
        index,
        2,
        UK,
        NO_LAG,
        CpiInterpolationType.Flat,
        settings,
        nominal=NOMINAL,
        **kwargs,
    ).build()


def _priced(
    settings: Settings,
    index: YoYInflationIndex,
    nominal: FlatForward,
    cap_floor_type: CapFloorType,
    distribution: str,
) -> float:
    """One instrument, one engine. An engine carries the arguments and results of
    the contract it last priced, so a cap and a floor never share one."""
    instrument = _cap_floor(settings, index, cap_floor_type, strike=STRIKE)
    engine = getattr(YoYInflationCapFloorEngine, distribution)(
        index, _surface(settings), nominal
    )
    assert engine.distribution() == distribution
    instrument.set_engine(engine)
    return instrument.npv()


def test_the_transcribed_fixture_is_the_one_the_oracle_runs():
    """The transcription guards, and the sharpest one is the base date: it is the
    curve's node zero, so a fixing schedule generated the wrong way round - or
    the sentinels dropped - would move every rate the optionlets are struck
    against, silently."""
    assert len(FIX_DATA) == 33
    assert FIX_DATA[-2:] == [-999.0, -999.0]
    assert len(YY_DATA) == 15

    schedule = _rpi_schedule()
    assert schedule.size() == 33
    assert schedule.dates()[0] == Date(4, 1, 2005)
    assert schedule.dates()[1] == Date(13, 1, 2005)
    assert schedule.dates()[-1] == Date(13, 8, 2007)

    _, index, _ = _market()
    assert index.underlying_index().last_fixing_date() == CURVE_BASE

    fresh = Settings()
    fresh.set_evaluation_date(TODAY)
    without_sentinels = ZeroInflationIndex.uk_rpi(fresh)
    for date, figure in zip(_rpi_schedule().dates(), FIX_DATA[:-2]):
        without_sentinels.add_fixing(date, figure)
    assert without_sentinels.last_fixing_date() == Date(1, 6, 2007), (
        "the two sentinels are what carry the base date from June to August; "
        "dropping them moves the curve's origin and every optionlet with it"
    )


def test_a_two_year_cap_and_floor_match_the_cached_black_values():
    """The load-bearing binding oracle. QuantLib's own tolerance is 0.02 on a
    notional of 1e6, which these values only reach if the curve, the base date,
    the zero observation lag and the Black distribution are all right at once."""
    settings, index, nominal = _market()

    cap = _priced(settings, index, nominal, CapFloorType.Cap, "black")
    floor = _priced(settings, index, nominal, CapFloorType.Floor, "black")

    print(f"black cap = {cap:.4f} (cached 219.452), floor = {floor:.4f} (cached 314.641)")
    assert abs(cap - 219.452) < 0.02
    assert abs(floor - 314.641) < 0.02


def test_the_displaced_and_normal_constructors_are_distinct_from_black():
    """The unit-displaced and Bachelier values sit two orders of magnitude above
    the Black ones on the same fixture, so a constructor wired to the wrong
    distribution cannot alias another. This is what pins the choice being made by
    the constructor rather than defaulted somewhere."""
    settings, index, nominal = _market()

    for distribution, cached_cap, cached_floor in [
        ("unit_displaced", 9114.61, 9209.8),
        ("bachelier", 8852.4, 8947.59),
    ]:
        cap = _priced(settings, index, nominal, CapFloorType.Cap, distribution)
        floor = _priced(settings, index, nominal, CapFloorType.Floor, distribution)

        print(f"{distribution} cap = {cap:.4f}, floor = {floor:.4f}")
        assert abs(cap - cached_cap) < 0.22
        assert abs(floor - cached_floor) < 0.22


def test_an_unset_strike_is_filled_at_the_money():
    """An unset strike is resolved off the nominal curve, and the rate that lands
    on the leg is the rate the instrument itself would reprice it at.

    The instrument's atm_rate bridges a handle to the bare curve reference the
    core takes, so this also pins that bridge: a broken one raises rather than
    returning a number."""
    settings, index, nominal = _market()
    cap = _cap_floor(settings, index, CapFloorType.Cap, atm_strike=nominal)

    print(f"atm strike = {cap.cap_rates()[0]:.10f}, 2y quote = {YY_DATA[1][1] / 100.0}")
    assert abs(cap.cap_rates()[0] - cap.atm_rate(nominal)) < 1e-12
    assert cap.coupon_count() == 2
    assert cap.floor_rates() == []


def test_the_derived_start_date_is_spot_at_zero_fixing_days():
    """The factory derives the start date rather than taking one: the evaluation
    date advanced by the fixing days under Following, plus the forward start.
    Both default to zero here, so the leg starts on the evaluation date itself
    and runs the two years out to an unadjusted anniversary. A non-zero default
    creeping into either would move every accrual and every fixing."""
    settings, index, _ = _market()
    cap = _cap_floor(settings, index, CapFloorType.Cap, strike=STRIKE)

    assert cap.start_date() == TODAY
    assert cap.maturity_date() == UK.advance(
        TODAY, 2, "Years", BusinessDayConvention.Unadjusted, False
    )


def test_a_strike_and_an_atm_curve_are_mutually_exclusive():
    """The core refuses both together and neither at all, at build time rather
    than at the setters: a consumed-self fluent setter returning Self cannot
    raise, so the refusal lands in build() and travels the binding as an
    ItofinError."""
    settings, index, nominal = _market()

    with pytest.raises(ItofinError) as both:
        _cap_floor(
            settings, index, CapFloorType.Cap, strike=STRIKE, atm_strike=nominal
        )
    assert "both given" in str(both.value)

    with pytest.raises(ItofinError) as neither:
        _cap_floor(settings, index, CapFloorType.Cap)
    assert "no strike and no ATM curve given" in str(neither.value)


def _raw_coupons(settings: Settings, index: YoYInflationIndex) -> list[YoYInflationCoupon]:
    """The two-year plain leg the Rust oracle hands its raw constructors
    (`inflationcapfloorengines.rs:528`), assembled here through the Python leg
    facade: annual from the evaluation date to an unadjusted two-year
    anniversary, unadjusted accruals, a ModifiedFollowing payment roll, the zero
    observation lag and a 1e6 notional.

    Two of those settings are transcribed for fidelity and are inert here, so
    neither is what this leg is pinned by. The generation rule cannot bite: two
    whole annual periods leave no stub, so Forward and Backward lay out the same
    three dates (and the Python facade already defaults to Forward). Nor can the
    payment roll: 13 August 2007, 2008 and 2009 are all UK business days, so
    Unadjusted, Following and ModifiedFollowing pay on the same days. What the
    cached values below do pin is the zero observation lag - two months instead
    misses them by about 47, per this file's header - and the 1e6 notional, which
    scales them outright.

    The coupons are bound once and reused across the constructors below. The leg
    rebuilds them on every call, so a second call would hand back different
    objects and the collar would no longer be written over the cap's leg."""
    end = UK.advance(TODAY, 2, "Years", BusinessDayConvention.Unadjusted, False)
    schedule = Schedule(
        TODAY,
        end,
        Frequency.Annual,
        UK,
        BusinessDayConvention.Unadjusted,
        DateGeneration.Forward,
    )
    return YoYInflationLeg(
        schedule,
        UK,
        index,
        NO_LAG,
        CpiInterpolationType.Flat,
        _day_counter(),
        notional=NOMINAL,
        payment_adjustment=BusinessDayConvention.ModifiedFollowing,
    ).coupons()


def _with_black_engine(
    instrument: YoYInflationCapFloor,
    settings: Settings,
    index: YoYInflationIndex,
    nominal: FlatForward,
) -> YoYInflationCapFloor:
    """One engine per instrument: an engine carries the arguments and results of
    the contract it last priced, so a cap, a floor and a collar never share
    one."""
    instrument.set_engine(
        YoYInflationCapFloorEngine.black(index, _surface(settings), nominal)
    )
    return instrument


def test_the_raw_constructors_reach_the_cached_black_values():
    """The load-bearing oracle for #859: the same two numbers the factory path
    reaches, but through the coupon vector Python now assembles itself. It is
    the accessor and the constructors that are under test - a coupon dropped in
    the hand-over, a strike vector landing in the wrong slot or a leg the
    instrument does not actually own would all miss 0.02 on a 1e6 notional,
    which is QuantLib's own tolerance.

    That tolerance is 1e-4 relative, though, and it is measured against a
    literal another build produced, so a convention slip could sit inside it.
    The sharper check is the last one: the same two numbers off the factory,
    in this process, on this curve. Both routes assemble the same coupons and
    run the same float sequence over them, so they agree exactly, and anything
    that moved this leg off the builder's would show there first."""
    settings, index, nominal = _market()
    coupons = _raw_coupons(settings, index)
    assert len(coupons) == 2

    cap = _with_black_engine(
        YoYInflationCapFloor.cap(coupons, [STRIKE], settings), settings, index, nominal
    )
    floor = _with_black_engine(
        YoYInflationCapFloor.floor(coupons, [STRIKE], settings), settings, index, nominal
    )
    cap_npv, floor_npv = cap.npv(), floor.npv()

    print(f"raw cap = {cap_npv:.4f} (cached 219.452), raw floor = {floor_npv:.4f} (cached 314.641)")
    assert abs(cap_npv - 219.452) < 0.02
    assert abs(floor_npv - 314.641) < 0.02
    assert cap.cap_rates() == [STRIKE, STRIKE]
    assert cap.floor_rates() == []

    assert cap_npv == _priced(settings, index, nominal, CapFloorType.Cap, "black")
    assert floor_npv == _priced(settings, index, nominal, CapFloorType.Floor, "black")


def test_a_collar_is_the_cap_less_the_floor():
    """A collar is long the cap and short the floor, so on one strike its value
    is the difference of the two priced separately. This is what the Collar
    enum arm buys: routed to Cap or Floor instead, the collar would price as one
    leg of the difference rather than both."""
    settings, index, nominal = _market()
    coupons = _raw_coupons(settings, index)

    collar = _with_black_engine(
        YoYInflationCapFloor.collar(coupons, [STRIKE], [STRIKE], settings),
        settings,
        index,
        nominal,
    )
    cap = _with_black_engine(
        YoYInflationCapFloor.cap(coupons, [STRIKE], settings), settings, index, nominal
    )
    floor = _with_black_engine(
        YoYInflationCapFloor.floor(coupons, [STRIKE], settings), settings, index, nominal
    )

    print(f"collar = {collar.npv():.6f}, cap - floor = {cap.npv() - floor.npv():.6f}")
    assert collar.npv() == pytest.approx(cap.npv() - floor.npv(), abs=1e-6)
    assert collar.cap_rates() == [STRIKE, STRIKE]
    assert collar.floor_rates() == [STRIKE, STRIKE]


def test_the_two_strike_vectors_keep_their_places():
    """The general constructor takes cap rates before floor rates, and a collar
    struck at one rate - what the parity check above builds - cannot tell the
    two apart. Two different strikes can: swapped, the collar would report them
    the other way round.

    The padding is visible here too: one strike given, one per coupon back."""
    settings, index, _ = _market()
    coupons = _raw_coupons(settings, index)

    collar = YoYInflationCapFloor.new(
        CapFloorType.Collar, coupons, [STRIKE], [0.01], settings
    )

    assert collar.cap_rates() == [STRIKE, STRIKE]
    assert collar.floor_rates() == [0.01, 0.01]


def test_the_single_vector_constructor_refuses_a_collar():
    """with_strikes carries one strike vector, which a collar cannot be built
    from: the core refuses it rather than guessing which side the vector is
    (`inflationcapfloor.rs:236`). A cap goes through and the vector lands on the
    cap rates, which is the arm the refusal is measured against."""
    settings, index, _ = _market()
    coupons = _raw_coupons(settings, index)

    with pytest.raises(ItofinError) as raised:
        YoYInflationCapFloor.with_strikes(
            CapFloorType.Collar, coupons, [STRIKE], settings
        )
    assert "only Cap/Floor types allowed" in str(raised.value)

    cap = YoYInflationCapFloor.with_strikes(
        CapFloorType.Cap, coupons, [STRIKE], settings
    )
    assert cap.cap_rates() == [STRIKE, STRIKE]
    assert cap.floor_rates() == []
