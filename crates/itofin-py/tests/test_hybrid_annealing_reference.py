"""Native Python hybrid annealing agrees with an independently frozen equation oracle."""

# standard library
import json
import math
from pathlib import Path

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin.optimize import Status, minimize

FIXTURE = Path(__file__).resolve().parents[2] / "itofin-optimize/tests/fixtures/hybrid_simulated_annealing.jsonl"
RECORDS = [json.loads(line) for line in FIXTURE.read_text().splitlines()]
INPUTS = [record for record in RECORDS if record["kind"] == "input"]
STATUS = {"converged": Status.ConvergedXTol, "max_iterations": Status.MaxIterations,
          "max_evaluations": Status.MaxEvaluations, "cancelled": Status.Cancelled, "nonfinite": Status.Nonfinite}


def reference_cost(name: str, point: np.ndarray) -> float:
    """Evaluate published analytic functions, never the oracle optimizer itself."""
    if name == "signed_quadratic":
        return float((point[0] - 1.25) ** 2 + 4.0 * (point[1] + 0.75) ** 2 - 3.0)
    if name == "boundary_quadratic":
        return float((point[0] - 3.0) ** 2 - 20.0)
    if name in ("constant", "rounded_normalization"):
        return -7.0
    if name == "multimodal_polynomial":
        return float((point[0] - 1.0) ** 2 * ((point[0] + 2.0) ** 2 + 0.1))
    if name == "himmelblau":
        return float((point[0] * point[0] + point[1] - 11.0) ** 2
                     + (point[0] + point[1] * point[1] - 7.0) ** 2)
    if name == "shifted_rastrigin":
        return float(15.0 + sum(v * v - 10.0 * math.cos(2.0 * math.pi * v) for v in point))
    raise AssertionError(name)


@pytest.mark.parametrize("record", INPUTS, ids=[record["name"] for record in INPUTS])
def test_independent_evaluation_callback_and_result_contract(record: dict) -> None:
    """Check exact counts and typed errors, then physical-point trajectories within float rounding."""
    case = record["value"]
    rows = [row for row in RECORDS if row.get("name") == record["name"]]
    expected = next(row["value"] for row in rows if row["kind"] == "result")
    expected_evaluations = [row["value"] for row in rows if row["kind"] == "evaluation"]
    expected_callbacks = [row["value"] for row in rows if row["kind"] == "callback"]
    evaluations: list[tuple[np.ndarray, float]] = []
    callbacks: list[np.ndarray] = []
    calls = 0
    error = ArithmeticError("frozen reference failure")

    def objective(point: np.ndarray) -> float:
        nonlocal calls
        calls += 1
        if case.get("fail_at_evaluation") == calls:
            evaluations.append((point.copy(), math.nan))
            raise error
        value = math.nan if case.get("nonfinite_at_evaluation") == calls else reference_cost(case["objective"], point)
        evaluations.append((point.copy(), value))
        return value

    def callback(point: np.ndarray) -> None:
        callbacks.append(point.copy())
        if case.get("fail_at_callback") == len(callbacks):
            raise error
        if case.get("cancel_after_cycle") == len(callbacks):
            raise StopIteration

    options = dict(case.get("options", {}))
    options.update({key: case[key] for key in ("maxiter", "maxfev") if key in case})
    result = None
    if expected["status"].endswith("_error"):
        with pytest.raises(ArithmeticError) as captured:
            minimize(objective, case["x0"], method="Hybrid-Simulated-Annealing", bounds=[tuple(pair) for pair in case["bounds"]],
                     options=options, callback=callback)
        assert captured.value is error
        assert calls == expected["nfev"]
    else:
        result = minimize(objective, case["x0"], method="Hybrid-Simulated-Annealing", bounds=[tuple(pair) for pair in case["bounds"]],
                          options=options, callback=callback)
        assert result.nit == expected["nit"] and result.nfev == expected["nfev"] == calls
        assert result.njev == expected["njev"] == 0
        assert result.status == STATUS[expected["status"]] and result.success == expected["success"]
        np.testing.assert_allclose(result.x, expected["x"], atol=1e-12, rtol=2e-13)
        if expected["fun"] is None:
            assert not math.isfinite(result.fun)
        else:
            assert result.fun == pytest.approx(expected["fun"], abs=1e-12, rel=2e-13)
    if record["trace"]:
        assert len(evaluations) == len(expected_evaluations)
        assert len(callbacks) == len(expected_callbacks)
        for (point, value), row in zip(evaluations, expected_evaluations):
            np.testing.assert_allclose(point, row["x"], atol=1e-12, rtol=2e-13)
            if row["fun"] is None:
                assert not math.isfinite(value)
            else:
                assert value == pytest.approx(row["fun"], abs=1e-12, rel=2e-13)
        for point, row in zip(callbacks, expected_callbacks):
            np.testing.assert_allclose(point, row["x"], atol=1e-12, rtol=2e-13)
    if "analytic_fun" in case and "global_gap" not in case:
        assert result is not None
        assert result.fun == pytest.approx(case["analytic_fun"], abs=2e-8)
    if "global_gap" in case:
        assert result is not None
        assert result.fun - case["analytic_fun"] > 0.9
