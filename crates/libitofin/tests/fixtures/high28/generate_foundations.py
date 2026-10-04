"""Replay retained deterministic covariance-conversion references."""

import QuantLib as ql

from generate import emit


def foundations():
    near_symmetric = ql.Matrix(
        [[1.0, 0.5, -0.25], [0.5 + 2e-13, 1.0, 0.75], [-0.25, 0.75, 1.0]]
    )
    converted = ql.getCovariance([0.2, 0.3, 0.4], near_symmetric)
    emit(
        "foundations-covariance.json",
        dict(
            standard_deviations=[0.2, 0.3, 0.4],
            near_symmetric_correlation=[list(row) for row in near_symmetric],
        ),
        dict(converted_covariance=[list(row) for row in converted]),
    )


if __name__ == "__main__":
    foundations()
