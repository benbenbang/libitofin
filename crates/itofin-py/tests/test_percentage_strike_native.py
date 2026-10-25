"""Independent same-grid QuantLib observations, separate from continuum checks."""

import json
import hashlib
import math
from pathlib import Path

import pytest

from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

DATA = (Path(__file__).parent / "fixtures" / "percentage_fd_reference.jsonl").read_bytes()
assert hashlib.sha256(DATA).hexdigest() == "d5748a30f7b95fef4c2bc92e9459c56c0b2839060d75c79f632d6d7f4af830b6"
ROWS = [json.loads(line) for line in DATA.decode().splitlines()]
assert len(ROWS) == 48


@pytest.mark.parametrize("row", ROWS)
def test_percentage_strike_native_prices_and_greeks(row):
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(
        float(row["S"]), float(row["r"]), float(row["q"]), float(row["sigma"]), today, DayCounter.actual360()
    )
    kind = OptionType.Call if row["kind"] == "call" else OptionType.Put
    option = VanillaOption.percentage_strike(kind, float(row["m"]), today + int(row["days"]), settings)
    engine = FdBlackScholesVanillaEngine(process, t_grid=int(row["grid"]), x_grid=int(row["grid"]))
    option.set_fd_engine(engine)
    for key, query in [("npv", option.npv), ("delta", option.delta), ("gamma", option.gamma), ("theta", option.theta)]:
        got, expected = query(), float(row[key])
        assert math.isfinite(got) and math.isfinite(expected)
        assert got == pytest.approx(expected, rel=0, abs=1e-7 if key == "theta" else 1e-8)
