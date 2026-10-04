"""Independent L2-star discrepancy, unit weights and bounded cube samples."""

import itertools
import json
import math
from fractions import Fraction
from pathlib import Path

import pytest

import itofin
from itofin import statistics


def exact_squared(samples):
    """Integrate squared empirical anchored-box error using rational products."""
    points = [[Fraction(value) for value in row] for row in samples]
    rows = len(points)
    dimension = len(points[0])
    pair_sum = sum(
        math.prod(1 - max(left, right) for left, right in zip(first, second))
        for first, second in itertools.product(points, repeat=2)
    )
    coordinate_sum = sum(math.prod(1 - value * value for value in point) for point in points)
    return pair_sum / rows**2 - Fraction(2, 2**dimension) * coordinate_sum / rows + Fraction(1, 3**dimension)


@pytest.mark.parametrize(
    "samples",
    [
        [[0.0, 0.0]],
        [[1.0, 1.0]],
        [[0.5, 0.5]],
        [[0.0, 1.0], [1.0, 0.0]],
        [[0.25, 0.5], [0.75, 0.25], [0.5, 0.75]],
        [[0.5, 0.5]] * 4,
        [[0.25, 0.5, 0.75], [0.75, 0.25, 0.5]],
        [[0.5] * 8],
    ],
)
def test_discrepancy_exact_reference(samples):
    """Single, duplicated, boundary and multidimensional points match exact integration."""
    expected = math.sqrt(float(exact_squared(samples)))
    assert statistics.discrepancy(samples) == pytest.approx(expected, rel=2e-12, abs=2e-12)
    assert statistics.discrepancy(samples, weights=[1.0] * len(samples)) == statistics.discrepancy(samples)


def test_discrepancy_unit_weight_and_owned_inputs():
    """Omitted and explicit unit weights agree without modifying caller sequences."""
    samples = [[0.25, 0.5], [0.75, 0.25], [0.5, 0.75]]
    before = [row[:] for row in samples]
    weights = [1.0, 1.0, 1.0]
    result = statistics.discrepancy(samples, weights=weights)
    assert samples == before
    assert weights == [1.0, 1.0, 1.0]
    samples[0][0] = 1.0
    weights[0] = 0.0
    assert result == pytest.approx(math.sqrt(float(exact_squared(before))), rel=2e-12, abs=2e-12)


@pytest.mark.parametrize("weight", [0.0, -0.0, -1.0, 0.5, 2.0, 1.0 + 2**-52, float("nan"), float("inf"), float("-inf")])
def test_discrepancy_rejects_nonunit_weights(weight):
    """Only exactly one is supported, not generic normalized or zero sample weights."""
    with pytest.raises(itofin.ItofinError, match="unit weights"):
        statistics.discrepancy([[0.5, 0.5], [0.25, 0.75]], weights=[1.0, weight])


@pytest.mark.parametrize("weights", [[], [1.0], [1.0, 1.0, 1.0]])
def test_discrepancy_rejects_weight_shape(weights):
    """Explicit weight arrays must match the number of points."""
    with pytest.raises(itofin.ItofinError):
        statistics.discrepancy([[0.5, 0.5], [0.25, 0.75]], weights=weights)


@pytest.mark.parametrize("coordinate", [-(2**-52), 1.0 + 2**-52, float("nan"), float("inf"), float("-inf")])
def test_discrepancy_rejects_coordinates_outside_closed_cube(coordinate):
    """Finite closed-cube validation includes later rows and components."""
    with pytest.raises(itofin.ItofinError):
        statistics.discrepancy([[0.25, 0.75], [0.5, coordinate]])


@pytest.mark.parametrize("samples", [[], [[]], [[0.5]], [[0.5, 0.5], [0.25]], [[0.5, 0.5], [0.25, 0.5, 0.75]]])
def test_discrepancy_rejects_empty_single_dimension_and_ragged(samples):
    """At least one rectangular point in two or more dimensions is required."""
    with pytest.raises(itofin.ItofinError):
        statistics.discrepancy(samples)


@pytest.mark.parametrize("rows,dimension", [(1, 257), (4097, 2), (626, 256), (4000, 256)])
def test_discrepancy_shape_and_pair_work_limits(rows, dimension):
    """Dimension, row, component and quadratic work limits reject before flattening."""
    with pytest.raises(itofin.ItofinError):
        statistics.discrepancy([[0.5] * dimension] * rows)


def test_discrepancy_dimension_limit_is_inclusive():
    """One point with 256 coordinates remains supported without arbitrary precision."""
    result = statistics.discrepancy([[1.0] * 256])
    assert math.isfinite(result)
    assert result == pytest.approx(math.sqrt(3.0**-256), rel=2e-12, abs=0.0)


def test_discrepancy_order_permutation_and_duplicate_contract():
    """Point and component permutations preserve the same unweighted measure."""
    samples = [[0.125, 0.5, 0.875], [0.75, 0.25, 0.5], [0.375, 0.625, 0.125]]
    value = statistics.discrepancy(samples)
    assert statistics.discrepancy(samples[::-1]) == pytest.approx(value, rel=2e-12, abs=2e-12)
    assert statistics.discrepancy([row[::-1] for row in samples]) == pytest.approx(value, rel=2e-12, abs=2e-12)
    assert statistics.discrepancy(samples * 2) == pytest.approx(value, rel=2e-12, abs=2e-12)


@pytest.mark.parametrize("measure", [statistics.discrepancy])
def test_discrepancy_sequence_extraction_and_keyword_contract(measure):
    """Concrete nested sequences are accepted, but input generators are not promised."""
    assert statistics.discrepancy(((0.5, 0.5),)) == statistics.discrepancy([[0.5, 0.5]])
    with pytest.raises(TypeError):
        measure([[0.5, 0.5]], [1.0])
    with pytest.raises(TypeError):
        measure(iter([[0.5, 0.5]]))
    with pytest.raises(TypeError):
        measure([iter([0.5, 0.5])])


def test_discrepancy_native_and_exact_fixtures():
    """Pinned QuantLib fixtures and independent exact integration agree with the binding."""
    root = Path(__file__).resolve().parents[3] / "sdk/go/testdata/discrepancy-statistics"
    manifest = json.loads((root / "oracle.json").read_text())
    for filename in manifest["cases"]:
        case = json.loads((root / filename).read_text())
        actual = statistics.discrepancy(case["samples"], weights=case["weights"])
        assert actual == pytest.approx(case["native_discrepancy"], rel=2e-12, abs=2e-12)
        assert actual == pytest.approx(case["exact_discrepancy"], rel=2e-12, abs=2e-12)
        squared = case["exact_squared"]
        rational = Fraction(int(squared["numerator"]), int(squared["denominator"]))
        assert exact_squared(case["samples"]) == rational
