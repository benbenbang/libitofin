"""QuantLib 1.43 normal and Hull-White lattice cap/floor oracles (#440)."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import IborLeg
from itofin.indexes import Euribor
from itofin.instruments import CapFloor
from itofin.models import CalibrationErrorType, CapHelper, HullWhite
from itofin.optimization import EndCriteria, LevenbergMarquardt
from itofin.pricingengines import BachelierCapFloorEngine, TreeCapFloorEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import ConstantOptionletVolatility, FlatForward, VolatilityType
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Period, Schedule


def fixture(rate=0.03, volatility=0.01):
    settings = Settings()
    settings.set_evaluation_date(Date(15, 1, 2026))
    dc = DayCounter.actual365_fixed()
    curve = FlatForward(Date(15, 1, 2026), rate, dc)
    index = Euribor.six_months(curve, settings)
    schedule = Schedule(
        Date(15, 1, 2027),
        Date(15, 1, 2030),
        Frequency.Semiannual,
        Calendar.target(),
        BDC.Unadjusted,
        termination_convention=BDC.Unadjusted,
    )
    leg = (
        IborLeg(schedule, index)
        .with_notional(100.0)
        .with_payment_day_counter(DayCounter.actual360())
        .with_payment_adjustment(BDC.Unadjusted)
        .with_fixing_days(0)
    )
    quote = SimpleQuote(volatility)
    engine = BachelierCapFloorEngine.with_flat_vol(curve, quote, dc, settings)
    return settings, curve, index, leg, quote, engine


def instruments(leg, settings, strike):
    return [
        CapFloor.cap(leg, [strike], settings),
        CapFloor.floor(leg, [strike - 0.015], settings),
        CapFloor.collar(leg, [strike], [strike - 0.015], settings),
    ]


@pytest.mark.parametrize(
    "rate,strike,vol,prices,vegas",
    [
        (
            0.03,
            0.04,
            0.01,
            [0.6034628980651433, 1.053253364838137, -0.4497904667729936],
            [127.78090454050886, 154.68272470527324, -26.901820164764377],
        ),
        (
            -0.01,
            -0.005,
            0.007,
            [0.6750024800533693, 0.29261507158291433, 0.38238740847045494],
            [164.10370412870708, 113.36514792045139, 50.73855620825567],
        ),
        (0.03, 0.04, 0.0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
    ],
)
def test_normal_quantlib_values_vega_and_decomposition(rate, strike, vol, prices, vegas):
    settings, _, _, leg, quote, engine = fixture(rate, vol)
    options = instruments(leg, settings, strike)
    for option, price, vega in zip(options, prices, vegas):
        option.set_bachelier_engine(engine)
        assert option.npv() == pytest.approx(price, rel=0, abs=1e-11)
        assert option.results().additional_results["vega"] == pytest.approx(vega, rel=0, abs=1e-10)
    assert options[2].npv() == pytest.approx(options[0].npv() - options[1].npv(), rel=0, abs=1e-12)
    if vol:
        quote.set_value(vol + 1e-6)
        up = options[0].npv()
        quote.set_value(vol - 1e-6)
        down = options[0].npv()
        assert (up - down) / 2e-6 == pytest.approx(vegas[0], rel=1e-7)


def test_normal_surface_retention_updates_and_error_recovery():
    settings, curve, index, leg, quote, engine = fixture()
    option = instruments(leg, settings, 0.04)[0]
    surface = ConstantOptionletVolatility(
        Date(15, 1, 2026),
        Calendar.null_calendar(),
        BDC.Following,
        0.01,
        DayCounter.actual365_fixed(),
        VolatilityType.Normal,
        0.0,
    )
    option.set_bachelier_engine(BachelierCapFloorEngine(surface, curve))
    assert option.npv() == pytest.approx(0.6034628980651433, rel=0, abs=1e-11)
    black = ConstantOptionletVolatility(
        Date(15, 1, 2026),
        Calendar.null_calendar(),
        BDC.Following,
        0.2,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        0.0,
    )
    with pytest.raises(ItofinError):
        BachelierCapFloorEngine(black, curve)
    option.set_bachelier_engine(engine)
    del engine, surface, black, curve, index, leg
    gc.collect()
    quote.set_value(0.02)
    assert option.npv() > 0.6034628980651433
    for invalid in [-0.01, float("nan"), float("inf")]:
        quote.set_value(invalid)
        with pytest.raises(ItofinError):
            option.npv()
    quote.set_value(0.01)
    assert option.npv() == pytest.approx(0.6034628980651433, rel=0, abs=1e-11)
    settings.set_evaluation_date(Date(16, 1, 2026))
    moved = option.npv()
    assert math.isfinite(moved) and moved < 0.6034628980651433
    settings.set_evaluation_date(Date(15, 1, 2026))
    assert option.npv() == pytest.approx(0.6034628980651433, rel=0, abs=1e-11)


def test_tree_quantlib_prices_fixed_grid_and_retention():
    settings, curve, index, leg, _, _ = fixture()
    model = HullWhite(curve, 0.05, 0.01)
    with pytest.raises(ItofinError):
        TreeCapFloorEngine(model, 0)
    engine = TreeCapFloorEngine(model, 100)
    options = instruments(leg, settings, 0.04)
    for option, expected in zip(options, [0.5187376052783954, 0.9526665495458903, -0.4339289442674949]):
        option.set_tree_engine(engine)
        assert option.npv() == pytest.approx(expected, rel=0, abs=1e-11)
    del model, engine, index, leg, curve
    gc.collect()
    settings.set_evaluation_date(Date(16, 1, 2026))
    assert options[0].npv() == pytest.approx(0.5187376052783954, rel=0, abs=1e-11)


def test_normal_cap_helper_tree_modes_and_invalid_inputs():
    settings, curve, index, _, quote, _ = fixture()

    def helper(length=5, shift=0.0):
        return CapHelper(
            Period(length, "Years"),
            quote,
            index,
            Frequency.Annual,
            DayCounter.actual365_fixed(),
            False,
            curve,
            CalibrationErrorType.RelativePriceError,
            VolatilityType.Normal,
            shift,
        )

    h = helper()
    market = h.market_value()
    assert market == pytest.approx(0.023719601355028135, rel=0, abs=1e-12)
    assert h.black_price(0.01) == pytest.approx(market, rel=0, abs=1e-14)
    with pytest.raises(ItofinError):
        h.model_value()
    model = HullWhite(curve, 0.05, 0.01)
    engine = TreeCapFloorEngine(model, 30)
    h.set_tree_engine(engine)
    before = h.model_value()
    assert before == pytest.approx(0.021512584929889333, rel=0, abs=1e-12)
    times = h.mandatory_times()
    assert len(times) > 1 and all(math.isfinite(t) and t >= 0 for t in times)
    h.set_tree_engine(TreeCapFloorEngine.with_time_grid(model, times))
    assert math.isfinite(h.model_value())
    for length, shift in [(0, 0), (-1, 0), (5, 0.1), (2**31 - 1, 0)]:
        with pytest.raises(ItofinError):
            helper(length, shift)
    for vol in [-0.1, float("nan"), float("inf")]:
        with pytest.raises(ItofinError):
            h.black_price(vol)
    quote.set_value(0.02)
    assert h.market_value() > market
    quote.set_value(0.01)
    assert h.market_value() == pytest.approx(market, rel=0, abs=1e-14)
    helpers = [helper(n) for n in [2, 3, 4, 5]]
    method = LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
    criteria = EndCriteria(10000, 100, 1e-6, 1e-8, 1e-8)
    model.calibrate_caps(helpers, method, criteria, True, 30)
    assert model.a() == pytest.approx(0.05)
    assert model.sigma() == pytest.approx(0.010705079712885954, rel=0, abs=1e-8)
    assert all(math.isfinite(x.calibration_error()) for x in helpers)


@pytest.mark.parametrize(
    "error_type",
    [CalibrationErrorType.RelativePriceError, CalibrationErrorType.PriceError, CalibrationErrorType.ImpliedVolError],
)
def test_cap_helper_live_invalid_quote_recovers(error_type):
    _, curve, index, _, quote, _ = fixture()
    helper = CapHelper(
        Period(5, "Years"),
        quote,
        index,
        Frequency.Annual,
        DayCounter.actual365_fixed(),
        False,
        curve,
        error_type,
        VolatilityType.Normal,
    )
    model = HullWhite(curve, 0.05, 0.01)
    times = helper.mandatory_times()
    engine = TreeCapFloorEngine.with_time_grid(model, times)
    helper.set_tree_engine(engine)
    expected = helper.calibration_error()
    assert math.isfinite(expected)
    times[:] = [float("nan")]
    del engine, model, curve, index
    gc.collect()
    for invalid in [float("nan"), float("inf"), -0.01]:
        quote.set_value(invalid)
        with pytest.raises(ItofinError):
            helper.calibration_error()
        quote.set_value(0.01)
        assert helper.calibration_error() == pytest.approx(expected, rel=0, abs=1e-14)
    assert math.isfinite(helper.model_value())


@pytest.mark.parametrize("shift,expected", [(0.0, 0.01372895806987519), (0.01, 0.018693918169825054)])
def test_cap_helper_shifted_lognormal_market_oracle(shift, expected):
    _, curve, index, _, quote, _ = fixture(volatility=0.2)
    helper = CapHelper(
        Period(5, "Years"),
        quote,
        index,
        Frequency.Annual,
        DayCounter.actual365_fixed(),
        False,
        curve,
        CalibrationErrorType.RelativePriceError,
        VolatilityType.ShiftedLognormal,
        shift,
    )
    assert helper.market_value() == pytest.approx(expected, rel=0, abs=1e-12)
