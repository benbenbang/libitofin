"""The inflation binding base: UK calendar, swap engine, index family (#748).

Self-contained. The index oracle mirrors the Rust one in
crates/libitofin/src/indexes/inflation/ukrpi.rs
(`a_loaded_history_reads_back_constant_within_each_period`, itself the UK RPI
block of `testZeroIndex` in inflation.cpp:230-311).
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import ZeroInflationIndex
from itofin.pricingengines import DiscountingSwapEngine
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter

TOLERANCE = 1e-12

JUNE_2007 = Date(1, 6, 2007)
JULY_2007 = Date(1, 7, 2007)
JUNE_FIXING = 206.2
JULY_FIXING = 207.3


def _uk_rpi_with_two_fixings() -> ZeroInflationIndex:
    """It is 13 September 2007 and UK RPI has published June and July, the last
    two figures of the `testZeroIndex` table (inflation.cpp:272-279).

    With a one-month availability lag the publication horizon is 1 August 2007,
    so both figures are far enough behind it to be read from history rather
    than forecast - which matters, since no curve is linked here.
    """
    settings = Settings()
    settings.set_evaluation_date(Date(13, 9, 2007))
    index = ZeroInflationIndex.uk_rpi(settings)
    index.add_fixing(JUNE_2007, JUNE_FIXING)
    index.add_fixing(JULY_2007, JULY_FIXING)
    return index


def test_united_kingdom_rolls_off_the_summer_bank_holiday():
    """27 August 2007 is a Monday and the UK Summer Bank Holiday
    (unitedkingdom.rs:50-51), which TARGET does not observe - so the roll to the
    28th discriminates the UK calendar from the one already exposed."""
    holiday = Date(27, 8, 2007)
    following = BusinessDayConvention.Following

    assert Calendar.united_kingdom().adjust(holiday, following) == Date(28, 8, 2007)
    assert Calendar.target().adjust(holiday, following) == holiday


def test_discounting_swap_engine_constructs_over_a_curve():
    """Construction smoke: the engine only stores the handle and the settings,
    so nothing here can fail. Its pricing is exercised where a swap exists."""
    settings = Settings()
    settings.set_evaluation_date(Date(13, 9, 2007))
    curve = FlatForward(Date(13, 9, 2007), 0.05, DayCounter.actual365_fixed())

    assert DiscountingSwapEngine(curve, settings) is not None


def test_a_published_figure_reads_back_across_its_whole_period():
    """Every day of the month a figure describes reads back that figure: the
    read quantizes its date to the start of the inflation period it falls in,
    rather than looking the day itself up."""
    index = _uk_rpi_with_two_fixings()

    assert abs(index.fixing(JUNE_2007, False) - JUNE_FIXING) < TOLERANCE
    assert abs(index.fixing(Date(17, 7, 2007), False) - JULY_FIXING) < TOLERANCE
    assert abs(index.fixing(Date(31, 7, 2007), False) - JULY_FIXING) < TOLERANCE


def test_a_figure_published_mid_period_is_spread_over_that_period():
    """add_fixing must route to the inflation base, which writes the figure to
    every day of its period, not to the Index trait default, which stores a
    single entry.

    Seeding on the 17th and reading on the 1st discriminates the two: the read
    quantizes to the period start, so a single entry filed on the 17th alone
    would be missed. Reading the period start via the two other tests' fixture
    cannot tell them apart, since there the write and the read land on the same
    day."""
    settings = Settings()
    settings.set_evaluation_date(Date(13, 9, 2007))
    index = ZeroInflationIndex.uk_rpi(settings)
    index.add_fixing(Date(17, 7, 2007), JULY_FIXING)

    assert abs(index.fixing(JULY_2007, False) - JULY_FIXING) < TOLERANCE
    assert abs(index.fixing(Date(31, 7, 2007), False) - JULY_FIXING) < TOLERANCE


def test_the_last_fixing_date_is_the_start_of_the_published_period():
    """The store holds the daily expansion, so its own last date is 31 July;
    the figure is attributed to the period's first day."""
    assert _uk_rpi_with_two_fixings().last_fixing_date() == JULY_2007


def test_needs_forecast_splits_at_the_publication_horizon():
    index = _uk_rpi_with_two_fixings()

    assert not index.needs_forecast(JUNE_2007)
    assert index.needs_forecast(Date(1, 6, 2010))


def test_a_forecast_without_a_linked_curve_raises():
    """The index is built over an empty relinkable handle, as the core's own
    constructor leaves it, so a forecast has nothing to dereference."""
    index = _uk_rpi_with_two_fixings()

    with pytest.raises(ItofinError, match="empty Handle"):
        index.fixing(Date(1, 6, 2010), True)


def test_the_three_named_indexes_construct_and_round_trip_a_fixing():
    """Each factory gets its own Settings: the fixing store is keyed by index
    name, and a shared store would let one index read another's history."""
    for factory, name in [
        (ZeroInflationIndex.uk_rpi, "UK RPI"),
        (ZeroInflationIndex.uk_hicp, "UK HICP"),
        (ZeroInflationIndex.eu_hicp, "EU HICP"),
    ]:
        settings = Settings()
        settings.set_evaluation_date(Date(13, 9, 2007))
        index = factory(settings)

        assert index.name() == name
        index.add_fixing(JUNE_2007, JUNE_FIXING)
        assert abs(index.fixing(JUNE_2007, False) - JUNE_FIXING) < TOLERANCE
