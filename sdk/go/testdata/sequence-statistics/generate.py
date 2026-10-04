"""Generate pinned native SequenceStatistics and exact centered references."""

import argparse
import hashlib
import json
import math
from fractions import Fraction
from pathlib import Path
import subprocess
import tempfile

COMMIT = "9863b578af0caa4cecabf697196533e84a8308b6"
SOURCES = [
    "ql/math/statistics/sequencestatistics.hpp",
    "ql/math/statistics/generalstatistics.hpp",
    "ql/math/statistics/generalstatistics.cpp",
    "ql/math/statistics/statistics.hpp",
    "ql/math/matrix.hpp",
    "ql/errors.cpp",
]
CASES = [
    ("unit_two_columns", [[1, 2], [2, 4], [3, 8], [4, 16]], None),
    ("unequal_weights", [[-2, 4, 1], [0, -3, 2], [5, 2, 7], [8, 9, 0]], [1, 2, 4, 3]),
    ("zero_weight_extrema", [[-100, 999, 5], [1, 4, 5], [3, 2, 5]], [0, 1, 3]),
    (
        "two_constant_columns",
        [[7, -3, 1, 4], [7, -3, 2, 2], [7, -3, 5, 6], [7, -3, 8, -1]],
        None,
    ),
    ("single_column", [[-4], [1], [9]], [2, 1, 3]),
    ("negative_correlation", [[-3, 6], [-1, 2], [2, -4], [5, -10]], [1, 4, 2, 3]),
    (
        "large_offset_cancellation",
        [[10**12 + i, 10**12 + 2 * i] for i in range(4)],
        None,
    ),
    ("zero_weight_count", [[1, -1], [3, 1], [3, 1]], [1, 1, 0]),
]


def centered(samples, weights):
    n, dimension = len(samples), len(samples[0])
    w = [Fraction(value) for value in weights]
    total = sum(w)
    mean = [
        sum(weight * Fraction(row[col]) for row, weight in zip(samples, w)) / total
        for col in range(dimension)
    ]
    covariance = [
        sum(
            weight * (Fraction(row[i]) - mean[i]) * (Fraction(row[j]) - mean[j])
            for row, weight in zip(samples, w)
        )
        / total
        * Fraction(n, n - 1)
        for i in range(dimension)
        for j in range(dimension)
    ]
    variance = [covariance[i * dimension + i] for i in range(dimension)]
    correlation = []
    for i in range(dimension):
        for j in range(dimension):
            if i == j or variance[i] == variance[j] == 0:
                value = 1.0
            elif variance[i] == 0 or variance[j] == 0:
                value = 0.0
            else:
                value = float(covariance[i * dimension + j]) / math.sqrt(
                    float(variance[i] * variance[j])
                )
            correlation.append(value)
    return dict(
        mean=list(map(float, mean)),
        variance=list(map(float, variance)),
        standard_deviation=[math.sqrt(float(value)) for value in variance],
        error_estimate=[math.sqrt(float(value) / n) for value in variance],
        minimum=[min(row[i] for row in samples) for i in range(dimension)],
        maximum=[max(row[i] for row in samples) for i in range(dimension)],
        covariance=list(map(float, covariance)),
        correlation=correlation,
    )


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quantlib", required=True, type=Path)
    parser.add_argument(
        "--boost-include", type=Path, default=Path("/opt/homebrew/include")
    )
    parser.add_argument("--cxx", default="clang++")
    parser.add_argument(
        "--output", type=Path, default=Path(__file__).with_name("oracle.json")
    )
    args = parser.parse_args()
    root = args.quantlib.resolve()
    commit = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if commit != COMMIT:
        raise ValueError(f"QuantLib commit is {commit}, expected {COMMIT}")
    hashes = {}
    for relative in SOURCES:
        path = root / relative
        pinned = subprocess.check_output(
            ["git", "-C", str(root), "show", f"{COMMIT}:{relative}"]
        )
        if pinned != path.read_bytes():
            raise ValueError(f"Modified QuantLib source: {relative}")
        hashes[relative] = digest(path)
    native_source = Path(__file__).with_name("native.cpp").resolve()
    compiler = subprocess.check_output([args.cxx, "--version"], text=True).splitlines()[
        0
    ]
    cases = []
    with tempfile.TemporaryDirectory(prefix="sequence-statistics-") as temporary:
        binary = Path(temporary) / "native"
        command = [
            args.cxx,
            "-std=c++17",
            "-O2",
            "-I",
            str(root),
            "-I",
            str(args.boost_include),
            str(native_source),
            str(root / "ql/errors.cpp"),
            str(root / "ql/math/statistics/generalstatistics.cpp"),
            "-o",
            str(binary),
        ]
        subprocess.run(command, check=True)
        for name, samples, optional_weights in CASES:
            weights = optional_weights or [1] * len(samples)
            lines = [f"{len(samples)} {len(samples[0])}"]
            lines.extend(
                " ".join(map(str, [weight, *row]))
                for row, weight in zip(samples, weights)
            )
            native = json.loads(
                subprocess.check_output(
                    [str(binary)], input="\n".join(lines), text=True
                )
            )
            reference = centered(samples, weights)
            for metric, values in native.items():
                if name == "large_offset_cancellation" and metric in {
                    "covariance",
                    "correlation",
                }:
                    continue
                for actual, expected in zip(values, reference[metric]):
                    if not math.isclose(actual, expected, rel_tol=2e-12, abs_tol=2e-12):
                        raise ValueError(f"{name}/{metric}: {actual} != {expected}")
            cases.append(
                dict(
                    name=name,
                    samples=samples,
                    weights=optional_weights,
                    native=native,
                    centered=reference,
                )
            )
    document = dict(
        provenance=dict(
            source_commit=COMMIT,
            source_sha256=hashes,
            compiler=compiler,
            compiler_flags=["-std=c++17", "-O2"],
            generator_sha256=digest(Path(__file__)),
            native_source_sha256=digest(native_source),
            native_reference="Actual QuantLib SequenceStatistics compiled from pinned sources",
            centered_reference="Independent exact fractions, weighted centered products, N/(N-1)",
        ),
        matrix_layout="row-major flat",
        ordinary_native_tolerance=dict(relative=2e-12, absolute=2e-12),
        cases=[f"{case['name']}.json" for case in cases],
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    for case in cases:
        (args.output.parent / f"{case['name']}.json").write_text(
            json.dumps(case, indent=2, sort_keys=True, allow_nan=False) + "\n"
        )
    args.output.write_text(
        json.dumps(document, indent=2, sort_keys=True, allow_nan=False) + "\n"
    )


if __name__ == "__main__":
    main()
