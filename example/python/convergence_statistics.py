"""Weighted mean checkpoints, not a convergence certificate."""

# standard library
import math

# itofin library
from itofin import statistics


def main():
    """Validate the checkpoint schedule and an incomplete final prefix."""
    observations = [2.0, 100.0, 8.0, 12.0]
    weights = [1.0, 0.0, 3.0, 2.0]
    accumulator = statistics.ConvergenceStatistics()
    accumulator.add_batch(observations, weights=weights)
    table = accumulator.convergence_table()
    assert table == [(1, 2.0), (3, 6.5)]
    assert table == statistics.convergence_table(observations, weights=weights)
    assert accumulator.samples() == 4
    assert math.isclose(accumulator.mean(), 25.0 / 3.0, abs_tol=1e-12)
    print("checkpoint table:", table)
    print("current mean:", accumulator.mean())
    accumulator.reset()
    assert accumulator.convergence_table() == []


if __name__ == "__main__":
    main()
