"""Exact ordered-pair oracle and binding ownership for maximum drawdown."""

# standard library
import json
import math
import sys
from fractions import Fraction
from pathlib import Path

# pypi/conda library
import pytest

# itofin library
import itofin
from itofin import statistics
from itofin.statistics import DrawdownResult, maximum_drawdown


def test_maximum_drawdown_independent_pairs_and_snapshots():
    """Exhaustive exact pairs independently reproduce hand-calculated fixtures."""
    path = Path(__file__).resolve().parents[3] / "sdk/go/testdata/drawdown.json"
    fixture = json.loads(path.read_text())
    assert len(fixture["cases"]) == 8
    assert maximum_drawdown is statistics.maximum_drawdown
    for case in fixture["cases"]:
        values = case["values"]
        best = (Fraction(0), 0, 0)
        for trough in range(len(values)):
            for peak in range(trough + 1):
                loss = max(Fraction(values[peak] - values[trough], values[peak]), Fraction(0))
                if loss > best[0]:
                    best = (loss, peak, trough)
        expected = case["expected"]
        assert (float(best[0]), best[1], best[2]) == (
            expected["drawdown"],
            expected["peak_index"],
            expected["trough_index"],
        )
        result = maximum_drawdown(values)
        assert isinstance(result, DrawdownResult)
        assert (result.drawdown, result.peak_index, result.trough_index) == (float(best[0]), best[1], best[2])
        values[0] = -99
        assert result.drawdown == float(best[0])
        with pytest.raises(AttributeError):
            setattr(result, "drawdown", -1)


def test_maximum_drawdown_errors_and_extreme_values():
    """Reject invalid values anywhere; finite tiny and huge NAVs remain supported."""
    for values in ([], [0], [-0.0], [-1], [math.nan], [math.inf], [-math.inf], [100, 1, 0], [100, 1, math.nan]):
        with pytest.raises(itofin.ItofinError):
            maximum_drawdown(values)
    tiny = math.ulp(0.0)
    for values, expected in (
        ([sys.float_info.max, sys.float_info.max / 2], 0.5),
        ([sys.float_info.max, tiny], 1.0),
        ([2 * tiny, tiny], 0.5),
        ([1, math.nextafter(1, 0)], 1 - math.nextafter(1, 0)),
    ):
        result = maximum_drawdown(values)
        assert (result.drawdown, result.peak_index, result.trough_index) == (expected, 0, 1)
    for _ in range(100):
        assert maximum_drawdown([100, 80]).drawdown == 0.2
