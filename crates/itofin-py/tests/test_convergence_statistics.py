"""Ordered cumulative means, bounded state and atomic Python mutations."""

import json
import math
from pathlib import Path

import pytest

import itofin
from itofin import statistics


def assert_table(actual, expected):
    """Check tuple shape, integer checkpoints and independent prefix means."""
    assert isinstance(actual, list)
    assert len(actual) == len(expected)
    for point, target in zip(actual, expected):
        assert isinstance(point, tuple)
        assert isinstance(point[0], int)
        assert point[0] == target[0]
        assert point[1] == pytest.approx(target[1], rel=2e-12, abs=2e-12)


@pytest.mark.parametrize("length", [0, 1, 2, 3, 6, 7, 8, 14, 15, 16, 31, 32])
@pytest.mark.parametrize("weighted", [False, True])
def test_convergence_checkpoints_and_batch(length, weighted):
    """Only complete doubling-plus-one prefixes are emitted in input order."""
    observations = [float(index - 8) for index in range(length)]
    weights = [float(index % 4 + 1) for index in range(length)] if weighted else None
    unit_weights = weights if weights is not None else [1.0] * length
    expected = []
    checkpoint = 1
    while checkpoint <= length:
        total = math.fsum(unit_weights[:checkpoint])
        mean = math.fsum(x * w for x, w in zip(observations[:checkpoint], unit_weights[:checkpoint])) / total
        expected.append((checkpoint, mean))
        checkpoint = 2 * checkpoint + 1
    assert_table(statistics.convergence_table(observations, weights=weights), expected)


def test_convergence_statistics_state_and_reset():
    """Mutable state agrees with batch, including an unrecorded final prefix."""
    accumulator = statistics.ConvergenceStatistics()
    assert accumulator.samples() == 0
    assert accumulator.weight_sum() == 0.0
    assert accumulator.convergence_table() == []
    with pytest.raises(itofin.ItofinError):
        accumulator.mean()
    accumulator.add(4.0)
    accumulator.add(100.0, 0.0)
    accumulator.add(-2.0, weight=2.0)
    accumulator.add_batch([10.0, 8.0], weights=[1.0, 3.0])
    assert accumulator.samples() == 5
    assert accumulator.weight_sum() == 7.0
    assert accumulator.mean() == pytest.approx(34 / 7)
    assert_table(accumulator.convergence_table(), [(1, 4.0), (3, 0.0)])
    assert_table(
        accumulator.convergence_table(),
        statistics.convergence_table([4.0, 100.0, -2.0, 10.0, 8.0], weights=[1.0, 0.0, 2.0, 1.0, 3.0]),
    )
    accumulator.add_batch([])
    assert accumulator.samples() == 5
    accumulator.reset()
    assert accumulator.samples() == 0
    assert accumulator.weight_sum() == 0.0
    assert accumulator.convergence_table() == []
    with pytest.raises(itofin.ItofinError):
        accumulator.mean()
    accumulator.add_batch([1.0, 2.0, 3.0])
    assert_table(accumulator.convergence_table(), [(1, 1.0), (3, 2.0)])


@pytest.mark.parametrize(
    "observations,weights",
    [
        ([1.0, float("nan")], None),
        ([1.0, float("inf")], None),
        ([1.0, float("-inf")], None),
        ([1.0, 2.0], []),
        ([1.0, 2.0], [1.0]),
        ([1.0], [float("nan")]),
        ([1.0], [float("inf")]),
        ([1.0], [-1.0]),
        ([1.0, 2.0], [1e308, 1e308]),
    ],
)
def test_convergence_statistics_batch_atomicity(observations, weights):
    """A late invalid value, weight or total leaves all state unchanged."""
    accumulator = statistics.ConvergenceStatistics()
    accumulator.add_batch([5.0, 7.0, 9.0])
    before = (accumulator.samples(), accumulator.weight_sum(), accumulator.mean(), accumulator.convergence_table())
    with pytest.raises(itofin.ItofinError):
        accumulator.add_batch(observations, weights=weights)
    assert (
        accumulator.samples(),
        accumulator.weight_sum(),
        accumulator.mean(),
        accumulator.convergence_table(),
    ) == before
    with pytest.raises(itofin.ItofinError):
        statistics.convergence_table(observations, weights=weights)


@pytest.mark.parametrize("value,weight", [(float("nan"), 1.0), (1.0, -1.0), (1.0, float("inf"))])
def test_convergence_single_add_atomicity(value, weight):
    """Rejected single updates preserve the current checkpoint snapshot."""
    accumulator = statistics.ConvergenceStatistics()
    accumulator.add(3.0)
    with pytest.raises(itofin.ItofinError):
        accumulator.add(value, weight)
    assert accumulator.samples() == 1
    assert accumulator.weight_sum() == 1.0
    assert accumulator.mean() == 3.0
    assert accumulator.convergence_table() == [(1, 3.0)]


def test_convergence_zero_weight_checkpoint_rejects_whole_batch():
    """A later positive total does not rescue an invalid first checkpoint."""
    accumulator = statistics.ConvergenceStatistics()
    for observations, weights in [([1.0], [0.0]), ([1.0, 2.0, 3.0], [0.0, 1.0, 1.0])]:
        with pytest.raises(itofin.ItofinError):
            accumulator.add_batch(observations, weights=weights)
        with pytest.raises(itofin.ItofinError):
            statistics.convergence_table(observations, weights=weights)
        assert accumulator.samples() == 0
        assert accumulator.weight_sum() == 0.0
        assert accumulator.convergence_table() == []
    accumulator.add(1.0)
    accumulator.add_batch([999.0, -999.0], weights=[0.0, 0.0])
    assert accumulator.samples() == 3
    assert accumulator.convergence_table() == [(1, 1.0), (3, 1.0)]


def test_convergence_owned_snapshots_and_inputs():
    """Returned lists and input mutations cannot mutate native accumulator state."""
    observations = [1.0, 2.0, 3.0]
    weights = [1.0, 1.0, 1.0]
    accumulator = statistics.ConvergenceStatistics()
    accumulator.add_batch(observations, weights=weights)
    snapshot = accumulator.convergence_table()
    snapshot[0] = (999, -999.0)
    snapshot.append((1000, 0.0))
    observations[0] = -999.0
    weights[0] = -999.0
    assert accumulator.mean() == 2.0
    assert accumulator.convergence_table() == [(1, 1.0), (3, 2.0)]
    accumulator.reset()
    assert snapshot == [(999, -999.0), (3, 2.0), (1000, 0.0)]


def test_convergence_numerical_cancellation_and_extreme_weights():
    """Stable core means preserve cancellation and small weighted contributions."""
    assert_table(statistics.convergence_table([1e16, -1e16, 1.0]), [(1, 1e16), (3, 1 / 3)])
    accumulator = statistics.ConvergenceStatistics()
    accumulator.add_batch([1e300, 0.0], weights=[1e-300, 1e300])
    assert accumulator.mean() == pytest.approx(1e-300, rel=2e-12, abs=0.0)
    assert accumulator.convergence_table() == [(1, 1e300)]


def test_convergence_length_limit_preserves_state():
    """The documented 100000-sample cap is inclusive and rejects extra updates."""
    observations = [2.0] * 100000
    accumulator = statistics.ConvergenceStatistics()
    accumulator.add_batch(observations)
    assert accumulator.samples() == 100000
    assert accumulator.mean() == 2.0
    assert [point[0] for point in accumulator.convergence_table()] == [2**power - 1 for power in range(1, 17)]
    with pytest.raises(itofin.ItofinError):
        accumulator.add(2.0)
    with pytest.raises(itofin.ItofinError):
        statistics.convergence_table(observations + [2.0])
    assert accumulator.samples() == 100000


@pytest.mark.parametrize("measure", [statistics.convergence_table])
@pytest.mark.parametrize("accumulator_type", [statistics.ConvergenceStatistics])
def test_convergence_sequence_extraction_and_keyword_contract(measure, accumulator_type):
    """Concrete sequences work; iterator-only inputs and positional batch weights do not."""
    assert statistics.convergence_table((1.0, 2.0, 3.0)) == [(1, 1.0), (3, 2.0)]
    with pytest.raises(TypeError):
        measure([1.0], [1.0])
    with pytest.raises(TypeError):
        measure(iter([1.0]))
    with pytest.raises(TypeError):
        accumulator_type().add_batch([1.0], [1.0])


def test_convergence_native_and_exact_fixtures():
    """Paired facade and accumulator reproduce independent native and rational fixtures."""
    root = Path(__file__).resolve().parents[3] / "sdk/go/testdata/convergence-statistics"
    manifest = json.loads((root / "oracle.json").read_text())
    for filename in manifest["cases"]:
        case = json.loads((root / filename).read_text())
        accumulator = statistics.ConvergenceStatistics()
        accumulator.add_batch(case["observations"], weights=case["weights"])
        for origin in ("native", "exact"):
            reference = case[origin]
            expected = [(point["samples"], point["mean"]) for point in reference["table"]]
            assert_table(statistics.convergence_table(case["observations"], weights=case["weights"]), expected)
            assert_table(accumulator.convergence_table(), expected)
            assert accumulator.samples() == reference["samples"]
            assert accumulator.weight_sum() == reference["weight_sum"]
            if reference["mean"] is None:
                with pytest.raises(itofin.ItofinError):
                    accumulator.mean()
            else:
                assert accumulator.mean() == pytest.approx(reference["mean"], rel=2e-12, abs=2e-12)
