"""Eonia conventions, retained dependencies and fixing errors."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Eonia, OvernightIndex
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter


def test_eonia_conventions_forecasts_lifetime_and_errors():
    """Zero lag and TARGET holidays distinguish the overnight conventions."""
    settings = Settings()
    today = Date(9, 10, 2015)
    settings.set_evaluation_date(today)
    index = Eonia(FlatForward(today, 0.04, DayCounter.actual360()), settings)
    assert isinstance(index, OvernightIndex)
    assert index.fixing_days() == 0
    assert index.currency().code() == "EUR"
    assert index.fixing_calendar().name == "TARGET"
    assert not index.fixing_calendar().is_business_day(Date(25, 12, 2015))
    expected = math.expm1(0.04 * 3 / 360) / (3 / 360)
    assert index.fixing(today, True) == pytest.approx(expected, rel=0, abs=1e-13)
    assert index.day_counter().year_fraction(today, Date(12, 10, 2015)) == 3 / 360
    empty = Eonia(None, settings)
    with pytest.raises(ItofinError):
        empty.fixing(Date(12, 10, 2015))
    with pytest.raises(ItofinError):
        index.fixing(Date(10, 10, 2015))
    with pytest.raises(ItofinError):
        index.fixing(Date(8, 10, 2015))
    del settings
    gc.collect()
    assert index.fixing(today, True) == pytest.approx(expected, rel=0, abs=1e-13)
