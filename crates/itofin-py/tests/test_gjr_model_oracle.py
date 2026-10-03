"""Independent QuantLib 1.43 GJR analytic, MC and calibration oracle fixtures."""

import json
from pathlib import Path

import pytest

from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import CalibrationErrorType, GJRGARCHModel, HestonModelHelper
from itofin.optimization import EndCriteria, Simplex
from itofin.pricingengines import AnalyticGJRGARCHEngine, MCEuropeanGJRGARCHEngine
from itofin.processes import GJRGARCHProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Calendar, Date, DayCounter, Period

FIXTURES = Path(__file__).resolve().parents[3] / "sdk/go/testdata"
REFERENCE = Date(15, 1, 2026)
SCHEMES = ("PartialTruncation", "FullTruncation", "Reflection")


def fixture(name):
    """Read a tracked independent native fixture."""
    return json.loads((FIXTURES / f"gjrgarch-model-{name}.json").read_text())


def setup(config, actual_actual=False):
    """Translate the oracle's observable process inputs without unit conversion."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    dc = DayCounter.actual_actual_isda() if actual_actual else DayCounter.actual365_fixed()
    quotes = [SimpleQuote(config[name]) for name in ("spot", "risk_free_rate", "dividend_yield")]
    curves = [FlatForward.from_quote(REFERENCE, quote, dc) for quote in quotes[1:]]
    process = GJRGARCHProcess(
        quotes[0],
        curves[0],
        curves[1],
        config["daily_variance"],
        config["omega"],
        config["alpha"],
        config["beta"],
        config["gamma"],
        config["lambda_parameter"],
        config["days_per_year"],
        SCHEMES[config["discretization"]],
    )
    return settings, dc, process, quotes


def option(settings, row, kind=OptionType.Call):
    """Construct the exact independent exercise date and strike."""
    return VanillaOption(kind, row["strike"], REFERENCE + row["maturity_days"], settings)


MATRIX = [
    (document["input"], row) for document in (fixture(f"matrix-{i}") for i in range(3)) for row in document["rows"]
]
ANALYTIC = fixture("analytic")
MC = fixture("mc")


@pytest.mark.parametrize("config,row", MATRIX)
@pytest.mark.parametrize("kind,field", [(OptionType.Call, "call"), (OptionType.Put, "put")])
def test_original_36_row_matrix_matches_current_native_oracle(config, row, kind, field):
    """Pair current native outputs rather than the rounded historical cache."""
    settings, _, process, _ = setup(config, actual_actual=True)
    engine = AnalyticGJRGARCHEngine(GJRGARCHModel(process))
    instrument = option(settings, row, kind)
    assert instrument.price_gjr(engine) == pytest.approx(row[field], rel=2e-12, abs=1e-10)


@pytest.mark.parametrize("row", ANALYTIC["rows"], ids=lambda row: row["name"])
@pytest.mark.parametrize("kind,field", [(OptionType.Call, "call"), (OptionType.Put, "put")])
def test_analytic_parameter_units_boundaries_and_rate_variants(row, kind, field):
    """Daily units, negative gamma/rate and nonstationarity retain source values."""
    settings, _, process, _ = setup({**ANALYTIC["input"], **row["changes"]})
    instrument = option(settings, row, kind)
    assert instrument.price_gjr(AnalyticGJRGARCHEngine(GJRGARCHModel(process))) == pytest.approx(
        row[field], rel=2e-12, abs=1e-10
    )


@pytest.mark.parametrize("row", MC["rows"])
def test_mc_all_schemes_seeded_price_and_standard_error_match_native(row):
    """Pair native seeded estimates, not a broad confidence interval."""
    settings, _, process, _ = setup(row["input"])
    config = row["settings"]
    engine = MCEuropeanGJRGARCHEngine(
        process,
        steps=config["timeSteps"],
        samples=config["requiredSamples"],
        seed=config["seed"],
        antithetic=config["antitheticVariate"],
    )
    instrument = option(settings, row, OptionType.Call if row["option_type"] == 1 else OptionType.Put)
    assert instrument.price_mc_gjr(engine) == pytest.approx(row["price"], rel=2e-12, abs=1e-11)
    assert instrument.error_estimate() == pytest.approx(row["error"], rel=2e-12, abs=1e-12)


def test_mc_tolerance_sampling_matches_independent_native_estimate():
    """The tolerance criterion and sample cap reproduce native sampling batches."""
    row = MC["tolerance"]
    settings, _, process, _ = setup(row["input"])
    config = row["settings"]
    instrument = option(settings, row)
    instrument.set_mc_gjr_engine(
        MCEuropeanGJRGARCHEngine(
            process,
            steps=config["timeSteps"],
            absolute_tolerance=config["requiredTolerance"],
            max_samples=config["maxSamples"],
            seed=config["seed"],
        )
    )
    assert instrument.npv() == pytest.approx(row["price"], rel=2e-12, abs=1e-11)
    assert instrument.error_estimate() == pytest.approx(row["error"], rel=2e-12, abs=1e-12)


def test_weighted_fixed_mask_calibration_matches_independent_synthetic_oracle():
    """The existing helper's implied-volatility residuals fit native market data."""
    data = fixture("synthetic")
    config = {**data["input"], "daily_variance": data["initial_params"][-1]}
    settings, dc, process, _ = setup(config)
    model = GJRGARCHModel(process)
    helpers = [
        HestonModelHelper(
            Period(row["maturity_days"], "Days"),
            Calendar.null_calendar(),
            config["spot"],
            row["strike"],
            row["market_vol"],
            config["risk_free_rate"],
            config["dividend_yield"],
            CalibrationErrorType.ImpliedVolError,
            REFERENCE,
            dc,
            settings,
        )
        for row in data["rows"]
    ]
    model.calibrate(
        helpers,
        Simplex(data["simplex_lambda"]),
        EndCriteria(data["max_iterations"], data["stationary_iterations"], *data["tolerances"]),
        weights=data["weights"],
        fix_parameters=data["fixed_params"],
    )
    assert model.params()[:5] == data["initial_params"][:5]
    assert model.daily_variance() == pytest.approx(data["calibrated_params"][-1], rel=0, abs=1e-10)
    weighted_sse = sum(weight * helper.calibration_error() ** 2 for weight, helper in zip(data["weights"], helpers))
    assert weighted_sse <= 1e-11
    engine = AnalyticGJRGARCHEngine(model)
    for row in data["rows"]:
        kind = OptionType.Put if row["strike"] < config["spot"] else OptionType.Call
        assert option(settings, row, kind).price_gjr(engine) == pytest.approx(row["target_price"], rel=0, abs=2e-7)


@pytest.mark.parametrize("config,row", MATRIX)
def test_original_36_row_mc_matrix_replays_native_sample_count(config, row):
    """Exact native counts avoid mistaking adaptive sampling drift for pricing."""
    settings, _, process, _ = setup(config, actual_actual=True)
    instrument = option(settings, row)
    instrument.set_mc_gjr_engine(
        MCEuropeanGJRGARCHEngine(
            process,
            steps_per_year=20,
            samples=row["mc_samples"],
            seed=1234,
        )
    )
    assert instrument.npv() == pytest.approx(row["mc_price"], rel=2e-12, abs=1e-11)
    assert instrument.error_estimate() == pytest.approx(row["mc_error"], rel=2e-12, abs=1e-12)


def test_live_bumps_match_fresh_native_values_not_native_carry_cache_defect():
    """The port correctly invalidates carry-dependent moments after live bumps."""
    settings, _, process, quotes = setup(ANALYTIC["input"])
    model = GJRGARCHModel(process)
    instrument = option(settings, {"strike": 100.0, "maturity_days": 90})
    instrument.set_gjr_engine(AnalyticGJRGARCHEngine(model))
    indices = {"spot": 0, "risk_free_rate": 1, "dividend_yield": 2}
    for row in ANALYTIC["cumulative_live_bumps"]:
        if row["name"] in indices:
            quotes[indices[row["name"]]].set_value(row["value"])
        elif row["name"] == "omega":
            params = model.params()
            params[0] = row["value"]
            model.set_params(params)
        assert instrument.npv() == pytest.approx(row["price"], rel=2e-12, abs=1e-10)
