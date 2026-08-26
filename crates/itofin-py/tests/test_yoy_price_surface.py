"""The year-on-year cap/floor term price surface binding (#909): the EU fixture
of `testYoYPriceSurfaceToATM` (`inflationvolatility.cpp`, `setup()` +
`setupPriceSurface()`), built through Python and read back against the values
the C++ suite caches.

This reproduces the core oracle in `yoycapfloortermpricesurface.rs` (module
tests, the 23-Nov-2007 fixture): the seven cached ATM year-on-year swap rates
are what discriminate the fixture being subtly wrong - the surface derives them
by solving for the strike where cap and floor prices cross, per maturity, and
then bootstrapping a year-on-year curve over them, so they exercise the whole
first-read calculation. A construction smoke test alone would prove nothing.

Fixture notes, both load-bearing.

1. The EUR nominal curve's node dates truncate the day fraction exactly as the
   C++ loop's integer casts do: the first node time, 0.0109589, is a hair under
   4/365, so the first curve date lands 3 days after the evaluation date, not
   4. Rounding instead moves every nominal discount off the C++ fixture.
2. No historical fixings are needed: at the 23-Nov-2007 evaluation date every
   fixing the derived bootstrap reads is forecast off the curve under
   construction, which is why the core oracle files none either.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex
from itofin.termstructures import YoYCapFloorTermPriceSurface, ZeroCurve
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

EVAL = Date(23, 11, 2007)

TIMES_EUR = [
    0.0109589, 0.0684932, 0.263014, 0.317808, 0.567123, 0.816438, 1.06575, 1.31507, 1.56438,
    2.0137, 3.01918, 4.01644, 5.01644, 6.01644, 7.01644, 8.01644, 9.02192, 10.0192, 12.0192,
    15.0247, 20.0301, 25.0356, 30.0329, 40.0384, 50.0466,
]

RATES_EUR = [
    0.0415600, 0.0426840, 0.0470980, 0.0458506, 0.0449550, 0.0439784, 0.0431887, 0.0426604,
    0.0422925, 0.0424591, 0.0421477, 0.0421853, 0.0424016, 0.0426969, 0.0430804, 0.0435011,
    0.0439368, 0.0443825, 0.0452589, 0.0463389, 0.0472636, 0.0473401, 0.0470629, 0.0461092,
    0.0450794,
]

C_STRIKES_EU = [0.02, 0.025, 0.03, 0.035, 0.04, 0.05]
F_STRIKES_EU = [-0.01, 0.00, 0.005, 0.01, 0.015, 0.02]
CF_MATURITY_YEARS = [3, 5, 7, 10, 15, 20, 30]

C_PRICES_EU = [
    [116.225, 204.945, 296.285, 434.29, 654.47, 844.775, 1132.33],
    [34.305, 71.575, 114.1, 184.33, 307.595, 421.395, 602.35],
    [6.37, 19.085, 35.635, 66.42, 127.69, 189.685, 296.195],
    [1.325, 5.745, 12.585, 26.945, 58.95, 94.08, 158.985],
    [0.501, 2.37, 5.38, 13.065, 31.91, 53.95, 96.97],
    [0.501, 0.695, 1.47, 4.415, 12.86, 23.75, 46.7],
]

F_PRICES_EU = [
    [0.501, 0.851, 2.44, 6.645, 16.23, 26.85, 46.365],
    [0.501, 2.236, 5.555, 13.075, 28.46, 44.525, 73.08],
    [1.025, 3.935, 9.095, 19.64, 39.93, 60.375, 96.02],
    [2.465, 7.885, 16.155, 31.6, 59.34, 86.21, 132.045],
    [6.9, 17.92, 32.085, 56.08, 95.95, 132.85, 194.18],
    [23.52, 47.625, 74.085, 114.355, 175.72, 229.565, 316.285],
]

SWAPS = [0.024586, 0.0247575, 0.0249396, 0.0252596, 0.0258498, 0.0262883, 0.0267915]
EPS = 2e-5


def _nominal_curve() -> ZeroCurve:
    dates = []
    for t in TIMES_EUR:
        ys = int(t)
        ds = int((t - ys) * 365)
        dates.append(Date(23, 11, 2007 + ys) + ds)
    return ZeroCurve(dates, RATES_EUR, DayCounter.actual365_fixed(), "Cubic")


def _price_surface() -> tuple[Settings, YoYCapFloorTermPriceSurface]:
    settings = Settings()
    settings.set_evaluation_date(EVAL)
    eu_hicp = ZeroInflationIndex.eu_hicp(settings)
    yoy_index = YoYInflationIndex.from_underlying(eu_hicp)
    surface = YoYCapFloorTermPriceSurface(
        0,
        Period(3, "Months"),
        yoy_index,
        CpiInterpolationType.Linear,
        _nominal_curve(),
        DayCounter.actual365_fixed(),
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
        C_STRIKES_EU,
        F_STRIKES_EU,
        [Period(n, "Years") for n in CF_MATURITY_YEARS],
        C_PRICES_EU,
        F_PRICES_EU,
        settings,
    )
    return settings, surface


def test_inspectors_read_the_eu_data_back():
    _settings, surface = _price_surface()
    assert surface.strikes() == pytest.approx(
        [-0.01, 0.00, 0.005, 0.01, 0.015, 0.02, 0.025, 0.03, 0.035, 0.04, 0.05]
    )
    assert surface.maturities() == [Period(n, "Years") for n in CF_MATURITY_YEARS]


def test_atm_yoy_swap_rates_reproduce_the_cached_swaps():
    _settings, surface = _price_surface()
    for n, expected in zip(CF_MATURITY_YEARS, SWAPS):
        rate = surface.atm_yoy_swap_rate(Date(23, 11, 2007 + n))
        assert rate == pytest.approx(expected, abs=EPS)


def test_prices_reproduce_the_quoted_grid_at_its_nodes():
    _settings, surface = _price_surface()
    for i in (0, 5):
        for j in (0, 6):
            date = Date(23, 11, 2007 + CF_MATURITY_YEARS[j])
            cap = surface.cap_price(date, C_STRIKES_EU[i])
            assert cap == pytest.approx(C_PRICES_EU[i][j], rel=1e-9)
            floor = surface.floor_price(date, F_STRIKES_EU[i])
            assert floor == pytest.approx(F_PRICES_EU[i][j], rel=1e-9)


def test_the_swap_rate_range_check_is_extrapolation_gated():
    _settings, surface = _price_surface()
    with pytest.raises(ItofinError):
        surface.atm_yoy_swap_rate(Date(23, 11, 2008), extrapolate=False)
