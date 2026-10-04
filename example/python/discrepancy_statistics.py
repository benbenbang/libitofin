"""Uncentered L2 coverage of four unit-square interior points."""

# standard library
import math

# itofin library
from itofin import statistics


def main():
    """Validate an independently computed exact squared discrepancy."""
    points = [[0.25, 0.25], [0.75, 0.75], [0.25, 0.75], [0.75, 0.25]]
    discrepancy = statistics.discrepancy(points)
    assert math.isclose(discrepancy, math.sqrt(71.0 / 4608.0), abs_tol=1e-12)
    print("uncentered L2 discrepancy:", discrepancy)


if __name__ == "__main__":
    main()
