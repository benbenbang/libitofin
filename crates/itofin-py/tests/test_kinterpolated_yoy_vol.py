"""The K-interpolated year-on-year optionlet volatility surface binding (#910):
`testYoYPriceSurfaceToVol` (`inflationvolatility.cpp:271-352`) reproduced from
Python, closing the #874 arc end-to-end.

This is the live numeric oracle of the core module
(`kinterpolatedyoyoptionletvol.rs`, the 23-Nov-2007 fixture): the facade builds
the whole stripping pipeline internally - the empty relinkable volatility
handle, the unit-displaced engine reading it and the linear stripper relinking
it each Brent iteration - and the `d_slice` lines at one and three years are
pinned against the `volATyear1[]`/`volATyear3[]` header literals to 1e-4. A
construction smoke test would prove nothing: the #387 trap this design guards
against (an engine on a handle the stripper never relinks) fails exactly here.

Fixture notes on top of the #909 surface fixture this file imports:

1. The yoyEU curve link is REQUIRED: the engine forecasts each optionlet's
   forward off the index's own year-on-year curve, while the stripper's generic
   index reads the price surface's bootstrapped one. Without the link the
   stripping raises the empty-handle error.
2. The curve's base date, 1-Oct-2007, is `setup()`'s
   `inflationPeriod(eval - 1 month, Monthly).first`; the thirty yearly nodes
   run off the 2-month-lagged cap start date, business-day adjusted.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.indexes import CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex
from itofin.termstructures import (
    InterpolatedYoYInflationCurve,
    KInterpolatedYoYOptionletVolatilitySurface,
    YoYCapFloorTermPriceSurface,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period
from test_yoy_price_surface import (
    C_PRICES_EU,
    C_STRIKES_EU,
    CF_MATURITY_YEARS,
    EVAL,
    F_PRICES_EU,
    F_STRIKES_EU,
    _nominal_curve,
)

YOY_EU_RATES = [
    0.0237951, 0.0238749, 0.0240334, 0.0241934, 0.0243567, 0.0245323, 0.0247213, 0.0249348,
    0.0251768, 0.0254337, 0.0257258, 0.0260217, 0.0263006, 0.0265538, 0.0267803, 0.0269378,
    0.0270608, 0.0271363, 0.0272, 0.0272512, 0.0272927, 0.027317, 0.0273615, 0.0273811,
    0.0274063, 0.0274307, 0.0274625, 0.027527, 0.0275952, 0.0276734, 0.027794,
]

VOL_AT_YEAR1 = [
    0.0129, 0.0094, 0.0083, 0.0073, 0.0064, 0.0058, 0.0042, 0.0046, 0.0053, 0.0064, 0.0098,
]
VOL_AT_YEAR3 = [
    0.0080, 0.0058, 0.0051, 0.0045, 0.0040, 0.0035, 0.0026, 0.0028, 0.0033, 0.0040, 0.0061,
]

EPS = 1e-4


def _vol_surface() -> tuple[Settings, KInterpolatedYoYOptionletVolatilitySurface]:
    settings = Settings()
    settings.set_evaluation_date(EVAL)
    eu_hicp = ZeroInflationIndex.eu_hicp(settings)
    yoy_index = YoYInflationIndex.from_underlying(eu_hicp)

    target = Calendar.target()
    mf = BusinessDayConvention.ModifiedFollowing
    cap_start = target.advance(EVAL, -2, "Months", mf, False)
    dates = [Date(1, 10, 2007)]
    for i in range(1, len(YOY_EU_RATES)):
        dates.append(target.advance(cap_start, i, "Years", mf, False))
    yoy_eu = InterpolatedYoYInflationCurve(
        EVAL, dates, YOY_EU_RATES, Frequency.Monthly, DayCounter.actual365_fixed()
    )
    yoy_index.link_to(yoy_eu)

    price_surface = YoYCapFloorTermPriceSurface(
        0,
        Period(3, "Months"),
        yoy_index,
        CpiInterpolationType.Linear,
        _nominal_curve(),
        DayCounter.actual365_fixed(),
        target,
        mf,
        C_STRIKES_EU,
        F_STRIKES_EU,
        [Period(n, "Years") for n in CF_MATURITY_YEARS],
        C_PRICES_EU,
        F_PRICES_EU,
        settings,
    )
    surface = KInterpolatedYoYOptionletVolatilitySurface(
        0,
        target,
        mf,
        DayCounter.actual365_fixed(),
        Period(3, "Months"),
        price_surface,
        yoy_index,
        _nominal_curve(),
        -0.5,
        settings,
    )
    return settings, surface


def test_d_slice_recovers_the_one_and_three_year_vol_lines():
    _settings, surface = _vol_surface()
    base = surface.base_date()

    strikes1, vols1 = surface.d_slice(Date(base.day, base.month, base.year + 1))
    assert len(strikes1) == 11
    assert vols1 == pytest.approx(VOL_AT_YEAR1, abs=EPS)

    strikes3, vols3 = surface.d_slice(Date(base.day, base.month, base.year + 3))
    assert len(strikes3) == 11
    assert vols3 == pytest.approx(VOL_AT_YEAR3, abs=EPS)


def test_strike_domain_and_max_date_read_the_price_surface_back():
    _settings, surface = _vol_surface()
    assert surface.min_strike() == pytest.approx(-0.01)
    assert surface.max_strike() == pytest.approx(0.05)
    assert surface.max_date() == Date(23, 11, 2037)


def test_volatility_interpolates_the_observed_slice():
    _settings, surface = _vol_surface()
    base = surface.base_date()
    year1 = Date(base.day, base.month, base.year + 1)
    quote_date = Calendar.target().advance(
        year1, 3, "Months", BusinessDayConvention.Unadjusted, False
    )
    vol = surface.volatility(quote_date, 0.03)
    assert vol == pytest.approx(VOL_AT_YEAR1[7], abs=EPS)
