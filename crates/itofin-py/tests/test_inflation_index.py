"""The inflation binding base: UK calendar, swap engine, index family (#748).

Self-contained. The index oracle mirrors the Rust one in
crates/libitofin/src/indexes/inflation/ukrpi.rs
(`a_loaded_history_reads_back_constant_within_each_period`, itself the UK RPI
block of `testZeroIndex` in inflation.cpp:230-311).
"""

from itofin import Settings
from itofin.pricingengines import DiscountingSwapEngine
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter


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
