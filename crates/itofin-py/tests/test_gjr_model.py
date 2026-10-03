"""GJR model parameter snapshots, observer propagation and retained markets."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import BermudanExercise, OptionType, VanillaOption
from itofin.models import GJRGARCHModel
from itofin.pricingengines import AnalyticGJRGARCHEngine
from itofin.processes import GJRGARCHProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter

REFERENCE = Date(15, 6, 2026)
EXPIRY = REFERENCE + 360
MARKET = (100.0, 0.03, 0.01)
PARAMS = (2e-6, 0.04, 0.88, 0.08, -0.4, 0.04 / 252)
NAMES = ("omega", "alpha", "beta", "gamma", "lambda_", "daily_variance")


def market(values=MARKET, params=PARAMS, scheme="FullTruncation", kind=OptionType.Call, strike=100.0):
    """Build a retained market, model, engine and European option."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    quotes = [SimpleQuote(value) for value in values]
    curves = [FlatForward.from_quote(REFERENCE, quote, DayCounter.actual360()) for quote in quotes[1:]]
    omega, alpha, beta, gamma, lambda_, v0 = params
    process = GJRGARCHProcess(quotes[0], curves[0], curves[1], v0, omega, alpha, beta, gamma, lambda_, scheme=scheme)
    model = GJRGARCHModel(process)
    engine = AnalyticGJRGARCHEngine(model)
    option = VanillaOption(kind, strike, EXPIRY, settings)
    option.set_gjr_engine(engine)
    return settings, quotes, curves, process, model, engine, option


@pytest.mark.parametrize("scheme", ["PartialTruncation", "FullTruncation", "Reflection"])
def test_model_process_resets_scheme_and_preserves_daily_units(scheme):
    """The model follows QuantLib FullTruncation regeneration semantics."""
    _, _, _, process, model, engine, option = market(scheme=scheme)
    assert model.params() == list(PARAMS)
    assert [getattr(model, name)() for name in NAMES] == list(PARAMS)
    assert process.discretization() == scheme
    derived = model.process()
    assert derived.discretization() == "FullTruncation"
    assert derived.days_per_year() == 252.0
    assert derived.initial_values() == [100.0, 0.04]
    assert derived.time(EXPIRY) == 1.0
    assert option.price_gjr(engine) == option.npv()
    assert math.isfinite(option.npv())


@pytest.mark.parametrize("index,value", list(enumerate((3e-6, 0.05, 0.86, 0.1, -0.2, 0.06 / 252))))
def test_parameter_updates_invalidate_price_but_keep_process_snapshots(index, value):
    """Rebuilt model pricing is separate from earlier process snapshots."""
    _, _, _, original_process, model, _, option = market()
    snapshot = model.process()
    before = option.npv()
    updated = model.params()
    updated[index] = value
    model.set_params(updated)
    assert not option.is_calculated()
    assert model.params() == updated
    assert [getattr(model.process(), name)() for name in NAMES] == updated
    assert [getattr(snapshot, name)() for name in NAMES] == list(PARAMS)
    assert [getattr(original_process, name)() for name in NAMES] == list(PARAMS)
    *_, rebuilt = market(params=updated)
    assert option.npv() == pytest.approx(rebuilt.npv(), rel=0, abs=1e-12)
    assert option.npv() != before
    updated[0] = 999.0
    assert model.omega() != updated[0]


@pytest.mark.parametrize("index,value", list(enumerate((105.0, 0.05, 0.02))))
def test_live_spot_and_curve_quotes_invalidate_cached_price(index, value):
    """All retained market handles notify the model and option."""
    _, quotes, _, _, model, _, option = market()
    before = option.npv()
    quotes[index].set_value(value)
    assert not option.is_calculated()
    updated = list(MARKET)
    updated[index] = value
    *_, rebuilt = market(values=updated)
    assert option.npv() == pytest.approx(rebuilt.npv(), rel=0, abs=1e-12)
    assert option.npv() != before
    assert model.process().initial_values()[0] == updated[0]


def test_option_retains_model_market_and_engine_after_python_collection():
    """Native ownership survives collection and still observes live inputs."""
    settings, quotes, curves, process, model, engine, option = market()
    retained_quote, retained_model = quotes[0], model
    del settings, quotes, curves, process, model, engine
    gc.collect()
    retained_quote.set_value(105.0)
    *_, rebuilt = market(values=(105.0, *MARKET[1:]))
    assert option.npv() == pytest.approx(rebuilt.npv(), rel=0, abs=1e-12)
    updated = retained_model.params()
    updated[-1] *= 1.2
    retained_model.set_params(updated)
    assert not option.is_calculated()
    *_, rebuilt = market(values=(105.0, *MARKET[1:]), params=updated)
    assert option.npv() == pytest.approx(rebuilt.npv(), rel=0, abs=1e-12)
    del retained_quote, retained_model
    gc.collect()
    assert math.isfinite(option.npv())


@pytest.mark.parametrize("index,value", [(i, value) for i in range(6) for value in (math.nan, math.inf, -math.inf)])
def test_nonfinite_model_parameters_rejected_atomically(index, value):
    """A rejected update leaves the original model and option intact."""
    _, _, _, _, model, _, option = market()
    before = option.npv()
    params = model.params()
    params[index] = value
    with pytest.raises(ItofinError):
        model.set_params(params)
    assert model.params() == list(PARAMS)
    assert option.npv() == before


@pytest.mark.parametrize(
    "params",
    [
        [],
        [0.1] * 5,
        [0.1] * 7,
        [0.0, *PARAMS[1:]],
        [*PARAMS[:-1], 0.0],
        [PARAMS[0], -0.1, *PARAMS[2:]],
        [PARAMS[0], 1.1, *PARAMS[2:]],
        [*PARAMS[:2], -0.1, *PARAMS[3:]],
        [*PARAMS[:2], 1.1, *PARAMS[3:]],
        [*PARAMS[:3], -1.1, *PARAMS[4:]],
        [*PARAMS[:3], 1.1, *PARAMS[4:]],
        [*PARAMS[:3], -0.1, *PARAMS[4:]],
    ],
)
def test_domain_and_shape_errors_preserve_model(params):
    """Dimension, scalar bounds and coupled coefficient guards are atomic."""
    _, _, _, _, model, _, option = market()
    before = option.npv()
    with pytest.raises(ItofinError):
        model.set_params(params)
    assert model.params() == list(PARAMS)
    assert option.npv() == before


@pytest.mark.parametrize(
    "index,value", [(0, 0.0), (0, -1.0), (0, math.nan), (0, math.inf), (1, math.nan), (2, math.inf)]
)
def test_invalid_live_market_recovers_without_notification_panic(index, value):
    """Market notifications defer invalid-input errors until pricing."""
    _, quotes, _, _, _, _, option = market()
    expected = option.npv()
    quotes[index].set_value(value)
    with pytest.raises(ItofinError):
        option.npv()
    quotes[index].set_value(MARKET[index])
    assert option.npv() == pytest.approx(expected, rel=0, abs=1e-12)


@pytest.mark.parametrize("greek", ["delta", "gamma", "theta", "vega", "rho", "dividend_rho", "error_estimate"])
def test_analytic_engine_exposes_npv_only(greek):
    """Missing Greeks and MC standard error are not silently fabricated."""
    *_, option = market()
    assert math.isfinite(option.npv())
    with pytest.raises(ItofinError):
        getattr(option, greek)()


@pytest.mark.parametrize("exercise", ["american", "bermudan"])
def test_analytic_engine_rejects_early_exercise(exercise):
    """Only European exercise is supported by the approximation."""
    settings, _, _, _, _, engine, _ = market()
    if exercise == "american":
        option = VanillaOption.american(OptionType.Call, 100.0, REFERENCE, EXPIRY, settings)
    else:
        option = VanillaOption.from_bermudan(
            OptionType.Call, 100.0, BermudanExercise([REFERENCE + 180, EXPIRY]), settings
        )
    option.set_gjr_engine(engine)
    with pytest.raises(ItofinError):
        option.npv()


def test_call_put_quantlib_source_convention():
    """Preserve QuantLib's undiscounted-dividend put-call convention."""
    *_, call = market(kind=OptionType.Call)
    *_, put = market(kind=OptionType.Put)
    expected = MARKET[0] - 100.0 * math.exp(-(MARKET[1] - MARKET[2]))
    assert call.npv() - put.npv() == pytest.approx(expected, rel=0, abs=2e-12)


@pytest.mark.parametrize("index,value", [(0, 0.0), (5, 0.0), (1, 1.1), (2, 1.1)])
def test_model_constructor_validates_model_domain_separately_from_process(index, value):
    """A valid process need not satisfy the stricter calibrated model bounds."""
    params = list(PARAMS)
    params[index] = value
    with pytest.raises(ItofinError):
        market(params=params)


@pytest.mark.parametrize("strike", [0.0, -1.0, math.nan, math.inf])
def test_analytic_engine_reports_invalid_strike(strike):
    """The analytic approximation requires a finite positive strike."""
    with pytest.raises(ItofinError):
        *_, instrument = market(strike=strike)
        instrument.npv()


def test_analytic_horizon_limit_and_one_day_boundary():
    """The bounded cubic approximation rejects large horizons and accepts one day."""
    settings, _, _, _, _, engine, _ = market()
    for maturity in (REFERENCE + 2000, REFERENCE + 1):
        instrument = VanillaOption(OptionType.Call, 100.0, maturity, settings)
        if maturity == REFERENCE + 1:
            assert math.isfinite(instrument.price_gjr(engine))
        else:
            with pytest.raises(ItofinError):
                instrument.price_gjr(engine)
