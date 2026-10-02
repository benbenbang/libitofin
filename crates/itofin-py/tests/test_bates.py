"""Bates live market, model mutation, lifetime and calibration contracts."""

import gc
import json
import math
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import BermudanExercise, OptionType, VanillaOption
from itofin.models import BatesModel, CalibrationErrorType, HestonModel, HestonModelHelper
from itofin.optimization import BoundaryConstraint, EndCriteria, LevenbergMarquardt, Simplex
from itofin.pricingengines import BatesEngine
from itofin.processes import BatesProcess, HestonProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Calendar, Date, DayCounter, Period

REFERENCE = Date(15, 6, 2026)
EXPIRY = REFERENCE + 360
MARKET = (100.0, 0.05, 0.02)
PARAMS = (0.04, 1.5, 0.04, 0.3, -0.7, 0.7, -0.1, 0.25)
MODEL_PARAMS = (PARAMS[2], PARAMS[1], PARAMS[3], PARAMS[4], PARAMS[0], *PARAMS[6:], PARAMS[5])
GREEKS = ("delta", "gamma", "theta", "vega", "rho", "dividend_rho")


def market(values=MARKET, params=PARAMS, kind=OptionType.Call, strike=100.0, order=144):
    """Market."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    quotes = [SimpleQuote(value) for value in values]
    dc = DayCounter.actual360()
    curves = [FlatForward.from_quote(REFERENCE, quote, dc) for quote in quotes[1:]]
    v0, kappa, theta, sigma, rho, intensity, nu, delta = params
    process = BatesProcess(quotes[0], curves[0], curves[1], v0, kappa, theta, sigma, rho, intensity, nu, delta)
    model = BatesModel(process)
    engine = BatesEngine(model, order)
    option = VanillaOption(kind, strike, EXPIRY, settings)
    option.set_bates_engine(engine)
    return settings, quotes, curves, process, model, engine, option


def helper(settings, strike=100.0, vol=0.25):
    """Helper."""
    return HestonModelHelper(
        Period(360, "Days"),
        Calendar.null_calendar(),
        MARKET[0],
        strike,
        vol,
        MARKET[1],
        MARKET[2],
        CalibrationErrorType.PriceError,
        REFERENCE,
        DayCounter.actual360(),
        settings,
    )


def test_getters_model_order_date_clock_and_retained_engine():
    """Getters model order date clock and retained engine."""
    _, _, _, process, model, engine, option = market()
    names = ("v0", "kappa", "theta", "sigma", "rho", "lambda_", "nu", "delta")
    assert [getattr(process, name)() for name in names] == list(PARAMS)
    assert [getattr(model, name)() for name in names] == list(PARAMS)
    assert model.params() == list(MODEL_PARAMS)
    assert process.spot() == MARKET[0]
    assert process.initial_values() == [MARKET[0], PARAMS[0]]
    assert process.time(REFERENCE) == 0.0
    assert process.time(EXPIRY) == 1.0
    assert option.npv() > 0.0
    assert option.price_bates(engine) == option.npv()


@pytest.mark.parametrize("kind", [OptionType.Call, OptionType.Put])
def test_zero_jump_matches_heston(kind):
    """Zero jump matches heston."""
    params = (*PARAMS[:5], 0.0, *PARAMS[6:])
    settings, _, _, _, _, _, option = market(params=params, kind=kind)
    heston = HestonModel(HestonProcess(MARKET[1], MARKET[2], MARKET[0], *PARAMS[:5], REFERENCE, DayCounter.actual360()))
    reference = VanillaOption(kind, 100.0, EXPIRY, settings)
    reference.set_heston_engine(heston, 144)
    assert option.npv() == pytest.approx(reference.npv(), rel=0, abs=1e-12)


def test_call_put_parity():
    """Call put parity."""
    *_, call = market(kind=OptionType.Call)
    *_, put = market(kind=OptionType.Put)
    expected = MARKET[0] * math.exp(-MARKET[2]) - 100.0 * math.exp(-MARKET[1])
    assert call.npv() - put.npv() == pytest.approx(expected, rel=0, abs=2e-12)


@pytest.mark.parametrize("index,value", enumerate((105.0, 0.07, 0.04)))
def test_live_spot_and_curves_invalidate_cached_value(index, value):
    """Live spot and curves invalidate cached value."""
    _, quotes, _, process, _, _, option = market()
    before = option.npv()
    assert option.is_calculated()
    quotes[index].set_value(value)
    assert not option.is_calculated()
    updated = list(MARKET)
    updated[index] = value
    *_, fresh = market(values=updated)
    after = option.npv()
    assert after != before
    assert after == pytest.approx(fresh.npv(), rel=0, abs=1e-12)
    assert process.spot() == updated[0]


@pytest.mark.parametrize("index,value", enumerate((0.06, 2.0, 0.4, -0.4, 0.05, -0.2, 0.35, 1.2)))
def test_every_model_parameter_invalidates_and_rebuilds(index, value):
    """Every model parameter invalidates and rebuilds."""
    _, _, _, process, model, _, option = market()
    before = option.npv()
    updated = model.params()
    updated[index] = value
    model.set_params(updated)
    assert not option.is_calculated()
    process_order = [updated[4], updated[1], updated[0], updated[2], updated[3], updated[7], *updated[5:7]]
    *_, rebuilt = market(params=process_order)
    assert option.npv() != before
    assert option.npv() == pytest.approx(rebuilt.npv(), rel=0, abs=1e-12)
    assert process.v0() == PARAMS[0]
    assert model.params() == updated
    updated[0] = 999.0
    assert model.theta() != 999.0


def test_model_and_option_retain_all_market_inputs_after_collection():
    """Model and option retain all market inputs after collection."""
    settings, quotes, curves, process, model, engine, option = market()
    expected = option.npv()
    retained_quote = quotes[0]
    retained_model = model
    del settings, quotes, curves, process, model, engine
    gc.collect()
    retained_quote.set_value(105.0)
    assert not option.is_calculated()
    *_, fresh = market(values=(105.0, *MARKET[1:]))
    assert option.npv() == pytest.approx(fresh.npv(), rel=0, abs=1e-12)
    updated = retained_model.params()
    updated[-1] = 1.0
    retained_model.set_params(updated)
    assert not option.is_calculated()
    assert option.npv() != expected
    del retained_quote, retained_model
    gc.collect()
    assert math.isfinite(option.npv())


@pytest.mark.parametrize("index,bad", [(i, x) for i in range(8) for x in (math.nan, math.inf)])
@pytest.mark.parametrize("target", ["process", "model"])
def test_nonfinite_parameters_rejected_and_model_update_atomic(index, bad, target):
    """Nonfinite parameters rejected and model update atomic."""
    if target == "process":
        params = list(PARAMS)
        params[index] = bad
        with pytest.raises(ItofinError):
            market(params=params)
    else:
        _, _, _, _, model, _, option = market()
        before = option.npv()
        params = model.params()
        params[index] = bad
        with pytest.raises(ItofinError):
            model.set_params(params)
        assert model.params() == list(MODEL_PARAMS)
        assert option.npv() == before


@pytest.mark.parametrize(
    "index,bad",
    [(0, 0.0), (1, 0.0), (2, 0.0), (3, 0.0), (4, -1.1), (4, 1.1), (5, -0.1), (7, -0.1), (6, 1000.0), (7, 1e200)],
)
def test_invalid_process_domains(index, bad):
    """Invalid process domains."""
    params = list(PARAMS)
    params[index] = bad
    with pytest.raises(ItofinError):
        market(params=params)


@pytest.mark.parametrize(
    "params", [[], [0.1] * 7, [0.1] * 9, [0.0, *MODEL_PARAMS[1:]], [*MODEL_PARAMS[:6], -0.1, MODEL_PARAMS[7]]]
)
def test_invalid_model_replacement_is_atomic(params):
    """Invalid model replacement is atomic."""
    _, _, _, _, model, _, option = market()
    before = option.npv()
    with pytest.raises(ItofinError):
        model.set_params(params)
    assert model.params() == list(MODEL_PARAMS)
    assert option.npv() == before


@pytest.mark.parametrize("index,bad", [(0, 0.0), (0, -1.0), (0, math.nan), (0, math.inf), (1, math.nan), (2, math.inf)])
def test_invalid_live_market_is_recoverable(index, bad):
    """Invalid live market is recoverable."""
    _, quotes, _, _, _, _, option = market()
    expected = option.npv()
    quotes[index].set_value(bad)
    with pytest.raises(ItofinError):
        option.npv()
    quotes[index].set_value(MARKET[index])
    assert option.npv() == pytest.approx(expected, rel=0, abs=1e-12)


@pytest.mark.parametrize("order", [0, 193])
def test_invalid_engine_order(order):
    """Invalid engine order."""
    with pytest.raises(ItofinError):
        market(order=order)


@pytest.mark.parametrize("greek", GREEKS)
def test_missing_greeks_raise(greek):
    """Missing greeks raise."""
    *_, option = market()
    assert option.npv() > 0.0
    with pytest.raises(ItofinError):
        getattr(option, greek)()


@pytest.mark.parametrize("kind", ["american", "bermudan"])
def test_non_european_exercise_rejected(kind):
    """Non european exercise rejected."""
    settings, _, _, _, _, engine, _ = market()
    if kind == "american":
        option = VanillaOption.american(OptionType.Call, 100.0, REFERENCE, EXPIRY, settings)
    else:
        exercise = BermudanExercise([REFERENCE + 180, EXPIRY])
        option = VanillaOption.from_bermudan(OptionType.Call, 100.0, exercise, settings)
    option.set_bates_engine(engine)
    with pytest.raises(ItofinError):
        option.npv()


@pytest.mark.parametrize(
    "params", [(*PARAMS[:5], 0.0, PARAMS[6], 0.0), (*PARAMS[:7], 0.0), (0.04, 0.1, 0.01, 0.8, -0.7, *PARAMS[5:])]
)
def test_zero_jump_limits_and_non_feller_model_supported(params):
    """Zero jump limits and non feller model supported."""
    _, _, _, _, model, _, option = market(params=params)
    assert math.isfinite(option.npv())
    model.set_params(model.params())
    assert math.isfinite(option.npv())


def test_calibration_empty_helpers_and_bad_options_rejected():
    """Calibration empty helpers and bad options rejected."""
    settings, _, _, _, model, _, _ = market()
    method = Simplex(0.1)
    criteria = EndCriteria(100, 20, 1e-8, 1e-8, 1e-8)
    with pytest.raises(ItofinError):
        model.calibrate([], method, criteria)
    invalid_options: tuple[dict[str, Any], ...] = (
        {"fix_parameters": [False] * 7},
        {"weights": [1.0, 2.0]},
        {"weights": [-1.0]},
        {"weights": [math.nan]},
        {"weights": [math.inf]},
    )
    for options in invalid_options:
        with pytest.raises(ItofinError):
            model.calibrate([helper(settings)], method, criteria, **options)
    with pytest.raises(TypeError):
        model.calibrate([helper(settings)], cast(Any, object()), criteria)
    with pytest.raises(TypeError):
        model.calibrate([helper(settings)], method, criteria, constraint=cast(Any, object()))


def test_calibration_fixed_mask_weights_and_constraint():
    """Calibration fixed mask weights and constraint."""
    settings, _, _, _, model, _, option = market()
    helpers = [helper(settings, strike) for strike in (90.0, 100.0, 110.0)]
    initial = model.params()
    before = option.npv()
    model.calibrate(
        helpers,
        LevenbergMarquardt(1e-8, 1e-8, 1e-8, False),
        EndCriteria(200, 30, 1e-8, 1e-8, 1e-8),
        constraint=BoundaryConstraint(-2.0, 3.0),
        weights=[1.0, 2.0, 1.0],
        fix_parameters=[True] * 4 + [False] + [True] * 3,
    )
    result = model.params()
    assert [result[i] for i in (0, 1, 2, 3, 5, 6, 7)] == [initial[i] for i in (0, 1, 2, 3, 5, 6, 7)]
    assert result[4] != initial[4]
    assert option.npv() != before
    assert all(math.isfinite(h.calibration_error()) for h in helpers)


def oracle_fixture():
    """Oracle fixture."""
    path = Path(__file__).resolve().parents[3] / "sdk/go/testdata/bates-oracle.json"
    return json.loads(path.read_text())


@pytest.mark.parametrize("row", oracle_fixture()["cases"], ids=lambda row: row[0])
def test_independent_quantlib_price_fixture(row):
    """Independent quantlib price fixture."""
    case = dict(zip(oracle_fixture()["columns"], row))
    reference = Date(*case["reference"])
    expiry = Date(*case["expiry"])
    day_counters = {
        "Actual360": DayCounter.actual360,
        "Actual365Fixed": DayCounter.actual365_fixed,
        "ActualActualISDA": DayCounter.actual_actual_isda,
    }
    dc = day_counters[case["day_counter"]]()
    settings = Settings()
    settings.set_evaluation_date(reference)
    theta, kappa, sigma, rho, v0, nu, delta, intensity = case["parameters"]
    process = BatesProcess(
        SimpleQuote(case["spot"]),
        FlatForward(reference, case["risk_free"], dc),
        FlatForward(reference, case["dividend"], dc),
        v0,
        kappa,
        theta,
        sigma,
        rho,
        intensity,
        nu,
        delta,
    )
    model = BatesModel(process)
    engine = BatesEngine(model, case["integration_order"])
    kind = OptionType.Call if case["kind"] == "call" else OptionType.Put
    option = VanillaOption(kind, case["strike"], expiry, settings)
    actual = option.price_bates(engine)
    assert actual == pytest.approx(case["price"], rel=0, abs=oracle_fixture()["price_absolute_tolerance"])


@pytest.mark.parametrize("fit", oracle_fixture()["calibration"]["fits"], ids=lambda fit: fit["name"])
def test_independent_quantlib_jump_calibration_fixture(fit):
    """Independent quantlib jump calibration fixture."""
    case = oracle_fixture()["calibration"]
    reference = Date(*case["reference"])
    settings = Settings()
    settings.set_evaluation_date(reference)
    dc = DayCounter.actual365_fixed()
    theta, kappa, sigma, rho, v0, nu, delta, intensity = fit["start"]
    process = BatesProcess(
        SimpleQuote(case["spot"]),
        FlatForward(reference, case["risk_free"], dc),
        FlatForward(reference, case["dividend"], dc),
        v0,
        kappa,
        theta,
        sigma,
        rho,
        intensity,
        nu,
        delta,
    )
    model = BatesModel(process)
    helpers = [
        HestonModelHelper(
            Period(months, "Months"),
            Calendar.null_calendar(),
            case["spot"],
            strike,
            volatility,
            case["risk_free"],
            case["dividend"],
            CalibrationErrorType.RelativePriceError,
            reference,
            dc,
            settings,
        )
        for months, strike, volatility, _ in case["helpers"]
    ]
    tolerance = fit["optimizer_tolerance"]
    model.calibrate(
        helpers,
        LevenbergMarquardt(tolerance, tolerance, tolerance, False),
        EndCriteria(fit["max_iterations"], fit["stationary_iterations"], tolerance, tolerance, tolerance),
        case["integration_order"],
        fix_parameters=fit["fixed"],
    )
    assert model.params() == pytest.approx(case["target"], rel=0, abs=case["parameter_tolerance"])
    errors = [abs(helper.calibration_error()) for helper in helpers]
    assert max(errors) < case["max_relative_error_tolerance"]
    for actual, seeded, fixed in zip(model.params(), fit["start"], fit["fixed"]):
        if fixed:
            assert actual == seeded


@pytest.mark.parametrize("bad", [0.0, -1.0, math.nan, math.inf])
def test_invalid_constructor_spot_rejected(bad):
    """Reject invalid spot already present at construction."""
    values = list(MARKET)
    values[0] = bad
    with pytest.raises(ItofinError):
        market(values=values)


def test_process_keyword_names_default_engine_and_owned_initial_values():
    """Expose Python-safe jump names and return independently owned state."""
    settings, quotes, curves, _, _, _, _ = market()
    process = BatesProcess(
        spot=quotes[0],
        risk_free=curves[0],
        dividend=curves[1],
        v0=PARAMS[0],
        kappa=PARAMS[1],
        theta=PARAMS[2],
        sigma=PARAMS[3],
        rho=PARAMS[4],
        lambda_=PARAMS[5],
        nu=PARAMS[6],
        delta=PARAMS[7],
    )
    values = process.initial_values()
    values[0] = -999.0
    assert process.initial_values() == [MARKET[0], PARAMS[0]]
    engine = BatesEngine(BatesModel(process))
    option = VanillaOption(OptionType.Call, 100.0, EXPIRY, settings)
    assert option.price_bates(engine) > 0.0


def test_invalid_optimizer_does_not_install_helper_engine():
    """Validate optimizer dispatch before mutating helper engine ownership."""
    settings, _, _, _, model, _, _ = market()
    unpriced = helper(settings)
    criteria = EndCriteria(100, 20, 1e-8, 1e-8, 1e-8)
    with pytest.raises(ItofinError):
        unpriced.calibration_error()
    with pytest.raises(TypeError):
        model.calibrate([unpriced], cast(Any, object()), criteria)
    with pytest.raises(ItofinError):
        unpriced.calibration_error()
    assert model.params() == list(MODEL_PARAMS)
