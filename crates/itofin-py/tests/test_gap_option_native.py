"""Same-grid native agreement is separate from inherited continuum/grid bias."""

import csv
import hashlib
import math
from pathlib import Path

import pytest

from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

FIXTURE = Path(__file__).parent / "fixtures" / "gap_fd_reference.csv"
DATA = FIXTURE.read_bytes()
assert hashlib.sha256(DATA).hexdigest() == "2d90130bb09080fae340d4fcd2743102a49244aee950b25efce05b7fa4d461b3"
ROWS = list(csv.DictReader(DATA.decode().splitlines()))
ORDINARY = [row for row in ROWS if all(float(row[key]) > 0 for key in ["S", "K", "sigma", "days"])]
assert len(ROWS) == 240 and len(ORDINARY) == 192


@pytest.mark.parametrize("row", ORDINARY, ids=lambda row: f"{row['id']}-grid{row['t_grid']}-damping{row['damping']}")
def test_gap_same_grid_native_prices_and_greeks(row):
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(
        float(row["S"]), float(row["r"]), float(row["q"]), float(row["sigma"]), today, DayCounter.actual360()
    )
    kind = OptionType.Call if row["kind"] == "call" else OptionType.Put
    option = VanillaOption.gap(kind, float(row["K"]), float(row["L"]), today + int(row["days"]), settings)
    engine = FdBlackScholesVanillaEngine(
        process, t_grid=int(row["t_grid"]), x_grid=int(row["x_grid"]), damping_steps=int(row["damping"])
    )
    option.set_fd_engine(engine)
    for key, query in [("npv", option.npv), ("delta", option.delta), ("gamma", option.gamma), ("theta", option.theta)]:
        got, expected = query(), float(row[key])
        assert math.isfinite(got) and math.isfinite(expected)
        assert got == pytest.approx(expected, rel=0, abs=1e-7 if key == "theta" else 1e-8)
