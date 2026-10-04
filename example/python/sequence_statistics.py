"""Weighted vector moments and paired covariance without NumPy or pandas."""

# standard library
import math

# itofin library
from itofin import statistics


def main():
    """Validate the paired weighted-vector example and print its results."""
    rows = [[1.0, 4.0], [3.0, 2.0], [100.0, -20.0]]
    weights = [1.0, 3.0, 0.0]
    mean = statistics.sequence_mean(rows, weights=weights)
    covariance = statistics.covariance_matrix(rows, weights=weights)
    correlation = statistics.correlation_matrix(rows, weights=weights)
    maximum = statistics.sequence_maximum(rows, weights=weights)
    assert mean == [2.5, 2.5]
    for actual, expected in zip(covariance, [[1.125, -1.125], [-1.125, 1.125]]):
        assert all(math.isclose(a, e, rel_tol=1e-12, abs_tol=1e-12) for a, e in zip(actual, expected))
    for actual, expected in zip(correlation, [[1.0, -1.0], [-1.0, 1.0]]):
        assert all(math.isclose(a, e, rel_tol=1e-12, abs_tol=1e-12) for a, e in zip(actual, expected))
    assert maximum == [100.0, 4.0]
    print("mean:", mean)
    print("covariance:", covariance)
    print("correlation:", correlation)
    print("maximum:", maximum)


if __name__ == "__main__":
    main()
