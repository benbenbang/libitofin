import csv
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

REFERENCE = Path(__file__).resolve().parent / "fixtures/asset-digital/asset_fd_reference.csv"


def reference_rows():
    with REFERENCE.open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    assert len(rows) == 64
    return rows


def asset_fd(row):
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(
        float(row["S"]), float(row["r"]), float(row["q"]), float(row["sigma"]), today, DayCounter.actual360()
    )
    engine = FdBlackScholesVanillaEngine(
        process, t_grid=int(row["t_grid"]), x_grid=int(row["x_grid"]), damping_steps=int(row["damping"])
    )
    kind = OptionType.Call if row["kind"] == "call" else OptionType.Put
    option = VanillaOption.asset_or_nothing(kind, float(row["K"]), today + int(row["days"]), settings)
    return option, engine


def ordinary(row):
    return int(row["days"]) > 0 and float(row["sigma"]) > 0 and float(row["K"]) > 0


def assert_native_fd(row):
    option, engine = asset_fd(row)
    price = option.price_fd(engine)
    got = [price, option.delta(), option.gamma(), option.theta()]
    for field, value in zip(["npv", "delta", "gamma", "theta"], got):
        want = float(row[field])
        assert math.isfinite(value) and math.isfinite(want)
        assert abs(value - want) <= (1e-6 if field == "theta" else 1e-8), (row["id"], row["x_grid"], field)
    assert option.npv() == price
    return price


def test_asset_digital_independent_native_fd_ordinary_and_convergence():
    rows = [row for row in reference_rows() if ordinary(row)]
    assert len(rows) == 40
    envelope = {100: 0.0, 200: 0.0, 400: 0.0, 800: 0.0}
    for row in rows:
        price = assert_native_fd(row)
        if int(row["days"]) == 360:
            grid = int(row["x_grid"])
            envelope[grid] = max(envelope[grid], abs(price - float(row["closed_form_npv"])))
    assert envelope[100] > envelope[200] > envelope[400] > envelope[800]
    assert envelope[800] < 0.001


def test_asset_digital_finite_edge_policy_not_continuum_convergence():
    rows = [row for row in reference_rows() if not ordinary(row)]
    assert len(rows) == 24
    for row in rows:
        assert_native_fd(row)


@pytest.mark.parametrize("kind", [OptionType.Call, OptionType.Put])
def test_asset_digital_zero_volatility_flat_mesher_is_native_error(kind):
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(100.0, 0.0, 0.0, 0.0, today, DayCounter.actual360())
    engine = FdBlackScholesVanillaEngine(process)
    option = VanillaOption.asset_or_nothing(kind, 100.0, today + 360, settings)
    with pytest.raises(ItofinError):
        option.price_fd(engine)
