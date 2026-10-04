"""Weighted vector moments, matrix conventions and bounded Python input contract."""

import json
import math
from pathlib import Path

import pytest

import itofin
from itofin import statistics


COMPONENTS = (
    statistics.sequence_mean,
    statistics.sequence_variance,
    statistics.sequence_standard_deviation,
    statistics.sequence_error_estimate,
    statistics.sequence_minimum,
    statistics.sequence_maximum,
)
MATRICES = (statistics.covariance_matrix, statistics.correlation_matrix)
MEASURES = COMPONENTS + MATRICES


def assert_matrix(actual, expected):
    """Compare nested rows without flattening away the returned shape."""
    assert isinstance(actual, list)
    assert len(actual) == len(expected)
    for row, target in zip(actual, expected):
        assert isinstance(row, list)
        assert row == pytest.approx(target, rel=1e-12, abs=1e-12)


def reference(samples, weights):
    """Compute weighted central products independently with a two-pass formula."""
    rows = len(samples)
    dimension = len(samples[0])
    total = math.fsum(weights)
    means = [math.fsum(w * row[c] for w, row in zip(weights, samples)) / total for c in range(dimension)]
    covariance = [
        [
            math.fsum(w * (row[i] - means[i]) * (row[j] - means[j]) for w, row in zip(weights, samples))
            / total
            * rows
            / (rows - 1)
            for j in range(dimension)
        ]
        for i in range(dimension)
    ]
    return means, covariance


@pytest.mark.parametrize("weights", [None, [1.0, 2.0, 1.0, 0.0]])
def test_component_statistics_and_manual_covariance(weights):
    """Vectors use independent component moments and count-corrected cross products."""
    samples = [[1.0, -2.0, 7.0], [3.0, 4.0, 8.0], [7.0, 1.0, 3.0], [-9.0, 20.0, -1.0]]
    expected_mean, covariance = reference(samples, weights or [1.0] * len(samples))
    expected_variance = [covariance[i][i] for i in range(3)]
    expected_std = [math.sqrt(value) for value in expected_variance]

    assert statistics.sequence_mean(samples, weights=weights) == pytest.approx(expected_mean)
    assert statistics.sequence_variance(samples, weights=weights) == pytest.approx(expected_variance)
    assert statistics.sequence_standard_deviation(samples, weights=weights) == pytest.approx(expected_std)
    assert statistics.sequence_error_estimate(samples, weights=weights) == pytest.approx(
        [value / math.sqrt(len(samples)) for value in expected_std]
    )
    assert statistics.sequence_minimum(samples, weights=weights) == [-9.0, -2.0, -1.0]
    assert statistics.sequence_maximum(samples, weights=weights) == [7.0, 20.0, 8.0]
    assert_matrix(statistics.covariance_matrix(samples, weights=weights), covariance)
    correlation = [[covariance[i][j] / (expected_std[i] * expected_std[j]) for j in range(3)] for i in range(3)]
    assert_matrix(statistics.correlation_matrix(samples, weights=weights), correlation)


def test_zero_weight_rows_count_but_extremes_remain_visible():
    """Zero-weight rows affect n-based correction and extrema, not weighted products."""
    samples = [[1.0, 2.0], [3.0, 6.0], [-1000.0, 1000.0]]
    weights = [1.0, 1.0, 0.0]
    assert statistics.sequence_mean(samples, weights=weights) == [2.0, 4.0]
    assert statistics.sequence_variance(samples, weights=weights) == pytest.approx([1.5, 6.0])
    assert statistics.sequence_error_estimate(samples, weights=weights) == pytest.approx(
        [math.sqrt(0.5), math.sqrt(2.0)]
    )
    assert statistics.sequence_minimum(samples, weights=weights) == [-1000.0, 2.0]
    assert statistics.sequence_maximum(samples, weights=weights) == [3.0, 1000.0]
    assert_matrix(statistics.covariance_matrix(samples, weights=weights), [[1.5, 3.0], [3.0, 6.0]])
    assert_matrix(statistics.correlation_matrix(samples, weights=weights), [[1.0, 1.0], [1.0, 1.0]])


def test_constant_columns_use_native_correlation_convention():
    """Two constant columns correlate at one; exactly one constant correlates at zero."""
    samples = [[5.0, -7.0, 1.0, 3.0], [5.0, -7.0, 2.0, 2.0], [5.0, -7.0, 3.0, 1.0]]
    expected = [[1.0, 1.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 0.0, -1.0, 1.0]]
    assert_matrix(statistics.correlation_matrix(samples), expected)
    covariance = statistics.covariance_matrix(samples)
    assert covariance[:2] == [[0.0] * 4, [0.0] * 4]
    assert_matrix(covariance[2:], [[0.0, 0.0, 1.0, -1.0], [0.0, 0.0, -1.0, 1.0]])


@pytest.mark.parametrize("measure", MEASURES)
def test_inputs_and_outputs_are_independent(measure):
    """Calls neither mutate inputs nor share mutable result rows across calls."""
    samples = [[1.0, 3.0], [2.0, 1.0], [4.0, 2.0]]
    before = [row[:] for row in samples]
    weights = [1.0, 2.0, 1.0]
    first = measure(samples, weights=weights)
    second = measure(samples, weights=weights)
    assert first == second
    if measure in MATRICES:
        first[0][0] = 987.0
        assert first[1][0] != 987.0
    else:
        first[0] = 987.0
    assert second == measure(samples, weights=weights)
    assert samples == before
    assert weights == [1.0, 2.0, 1.0]


@pytest.mark.parametrize("measure", MEASURES)
def test_tuples_and_numeric_arrays_match_lists(measure):
    """Existing sequence extraction accepts tuples and NumPy arrays without extra dependencies."""
    numpy = pytest.importorskip("numpy")
    samples = [[1.0, 2.0], [3.0, 1.0], [2.0, 6.0]]
    weights = [1.0, 2.0, 1.0]
    expected = measure(samples, weights=weights)
    assert measure(tuple(tuple(row) for row in samples), weights=tuple(weights)) == expected
    assert measure(numpy.array(samples), weights=numpy.array(weights)) == expected


@pytest.mark.parametrize("measure", MEASURES)
@pytest.mark.parametrize("samples", [[], [[]], [[1.0], []], [[1.0, 2.0], [3.0]], [[1.0], [2.0, 3.0]]])
def test_empty_or_ragged_rows_fail(measure, samples):
    """Shape errors surface as ItofinError before allocating a flattened copy."""
    with pytest.raises(itofin.ItofinError):
        measure(samples)


@pytest.mark.parametrize("measure", MEASURES)
@pytest.mark.parametrize("bad", [math.nan, math.inf, -math.inf])
def test_nonfinite_values_fail_even_in_zero_weight_rows(measure, bad):
    """Every input value is finite, including data ignored by weighted moments."""
    with pytest.raises(itofin.ItofinError):
        measure([[1.0, 2.0], [bad, 3.0]], weights=[1.0, 0.0])


@pytest.mark.parametrize("measure", MEASURES)
@pytest.mark.parametrize(
    "weights", [[], [1.0], [1.0, 2.0, 3.0], [0.0, 0.0], [-1.0, 2.0], [math.nan, 1.0], [math.inf, 1.0]]
)
def test_invalid_weights_fail(measure, weights):
    """Weights match row count, are finite and nonnegative, and have positive total."""
    with pytest.raises(itofin.ItofinError):
        measure([[1.0, 2.0], [3.0, 4.0]], weights=weights)


@pytest.mark.parametrize(
    "measure", [statistics.sequence_mean, statistics.sequence_minimum, statistics.sequence_maximum]
)
def test_single_row_component_results(measure):
    """Single-row center and extrema remain defined."""
    assert measure([[2.0, -3.0]], weights=[2.0]) == [2.0, -3.0]


@pytest.mark.parametrize("measure", COMPONENTS[1:4] + MATRICES)
def test_single_row_dispersion_is_rejected(measure):
    """Dispersion and both matrices require at least two counted rows."""
    with pytest.raises(itofin.ItofinError):
        measure([[2.0, -3.0]])


def test_units_and_common_weight_rescaling():
    """Means follow units, covariance follows products, and correlations are dimensionless."""
    samples = [[1.0, -2.0], [3.0, 4.0], [7.0, 1.0]]
    weights = [1.0, 2.0, 3.0]
    transformed = [[row[0] * 10.0, row[1] * -2.0] for row in samples]
    covariance = statistics.covariance_matrix(samples, weights=weights)
    scaled = statistics.covariance_matrix(transformed, weights=weights)
    assert_matrix(
        scaled, [[covariance[0][0] * 100, covariance[0][1] * -20], [covariance[1][0] * -20, covariance[1][1] * 4]]
    )
    correlation = statistics.correlation_matrix(samples, weights=weights)
    assert_matrix(
        statistics.correlation_matrix(transformed, weights=weights),
        [[1.0, -correlation[0][1]], [-correlation[1][0], 1.0]],
    )
    for measure in MEASURES:
        actual = measure(samples, weights=[weight * 2.0 for weight in weights])
        expected = measure(samples, weights=weights)
        if measure in MATRICES:
            assert_matrix(actual, expected)
        else:
            assert actual == pytest.approx(expected)


@pytest.mark.parametrize("measure", MEASURES)
def test_keyword_only_weights_and_bad_element_types(measure):
    """Weights are keyword-only and nonnumeric samples do not enter Rust calculations."""
    with pytest.raises(TypeError):
        measure([[1.0], [2.0]], [1.0, 1.0])
    with pytest.raises(TypeError):
        measure([[object()], [2.0]])


def test_shape_limits_and_matrix_work_limit():
    """Row, dimension, total value and matrix work bounds are distinct contracts."""
    for samples in ([[0.0]] * 100_001, [[0.0] * 257] * 2, [[0.0] * 256] * 3907):
        with pytest.raises(itofin.ItofinError):
            statistics.sequence_mean(samples)
    samples = [[1.0] * 256] * 1526
    assert statistics.sequence_mean(samples) == [1.0] * 256
    for measure in MATRICES:
        with pytest.raises(itofin.ItofinError):
            measure(samples)


def test_extreme_finite_zero_weight_observations_do_not_poison_products():
    """Finite ignored extremes remain visible only in component extrema."""
    samples = [[1.0, 2.0], [3.0, 6.0], [1e308, -1e308]]
    weights = [1.0, 1.0, 0.0]
    assert statistics.sequence_mean(samples, weights=weights) == [2.0, 4.0]
    assert statistics.sequence_maximum(samples, weights=weights) == [1e308, 6.0]
    assert statistics.sequence_minimum(samples, weights=weights) == [1.0, -1e308]
    assert_matrix(statistics.covariance_matrix(samples, weights=weights), [[1.5, 3.0], [3.0, 6.0]])


@pytest.mark.parametrize("measure", MEASURES)
def test_generators_are_not_accepted_as_sequences(measure):
    """Iterator-only inputs are rejected consistently with existing typed sequence facades."""
    with pytest.raises(TypeError):
        measure((row for row in [[1.0], [2.0]]))
    with pytest.raises(TypeError):
        measure([[1.0], [2.0]], weights=(weight for weight in [1.0, 1.0]))
    with pytest.raises(TypeError):
        measure([(value for value in [1.0]), [2.0]])


@pytest.mark.parametrize("measure", MEASURES)
def test_overflowing_total_weight_is_rejected(measure):
    """A nonfinite total weight is rejected even when each weight is finite."""
    with pytest.raises(itofin.ItofinError):
        measure([[1.0], [2.0]], weights=[1e308, 1e308])


@pytest.mark.parametrize("measure", COMPONENTS[1:4] + MATRICES)
def test_nonfinite_dispersion_result_is_rejected(measure):
    """Finite inputs do not authorize overflowing dispersion output."""
    with pytest.raises(itofin.ItofinError):
        measure([[-1e308], [1e308]])


def test_independent_native_and_centered_oracles():
    """All functions match independent fixtures, preserving documented cancellation divergence."""
    data = Path(__file__).resolve().parents[3] / "sdk/go/testdata/sequence-statistics"
    manifest = json.loads((data / "oracle.json").read_text())
    functions = dict(
        zip(
            (
                "mean",
                "variance",
                "standard_deviation",
                "error_estimate",
                "minimum",
                "maximum",
                "covariance",
                "correlation",
            ),
            MEASURES,
        )
    )
    for filename in manifest["cases"]:
        case = json.loads((data / filename).read_text())
        for name, measure in functions.items():
            actual = measure(case["samples"], weights=case["weights"])
            if measure in MATRICES:
                dimension = len(case["samples"][0])
                assert len(actual) == dimension
                flattened: list[float] = []
                for row in actual:
                    assert isinstance(row, list)
                    assert len(row) == dimension
                    flattened.extend(row)
                actual = flattened
            assert actual == pytest.approx(case["centered"][name], rel=1e-12, abs=1e-12), (case["name"], name)
            if case["name"] != "large_offset_cancellation" or name not in ("covariance", "correlation"):
                assert actual == pytest.approx(case["native"][name], rel=2e-12, abs=2e-12), (case["name"], name)
