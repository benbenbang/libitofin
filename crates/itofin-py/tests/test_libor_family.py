"""The Libor index family facades and the JPY currency.

Each family assertion pins a value the core oracles pin (usdlibor.rs /
jpylibor.rs / gbplibor.rs), so a facade wired to the wrong family constructor
fails on the composed name, currency or day-count suffix. The USD value-date
and maturity fixtures land on Veterans Day under the joint UK-plus-US
calendar, so a base half built off anything but the real core Libor index
keeps the 11th and fails.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.indexes import Currency, GbpLibor, IborIndex, JpyLibor, UsdLibor
from itofin.time import Date, Period


@pytest.fixture
def settings():
    settings = Settings()
    settings.set_evaluation_date(Date(1, 1, 2020))
    return settings


def test_usd_libor_carries_the_family_configuration(settings):
    index = UsdLibor(Period(3, "Months"), None, settings)

    assert isinstance(index, IborIndex)
    assert index.name() == "USDLibor3M Actual/360"
    assert index.tenor() == Period(3, "Months")
    assert index.currency().code() == "USD"


def test_jpy_libor_carries_the_family_configuration(settings):
    index = JpyLibor(Period(3, "Months"), None, settings)

    assert isinstance(index, IborIndex)
    assert index.name() == "JPYLibor3M Actual/360"
    assert index.tenor() == Period(3, "Months")
    assert index.currency().code() == "JPY"


def test_gbp_libor_carries_the_family_configuration(settings):
    index = GbpLibor(Period(3, "Months"), None, settings)

    assert isinstance(index, IborIndex)
    assert index.name() == "GBPLibor3M Actual/365 (Fixed)"
    assert index.tenor() == Period(3, "Months")
    assert index.currency().code() == "GBP"


def test_currency_jpy_carries_the_iso_code():
    assert Currency.jpy().code() == "JPY"


def test_usd_libor_rolls_value_and_maturity_on_the_joint_calendar(settings):
    index = UsdLibor(Period(3, "Months"), None, settings)

    assert index.value_date(Date(7, 11, 2019)) == Date(12, 11, 2019)
    assert index.maturity_date(Date(11, 8, 2020)) == Date(12, 11, 2020)
