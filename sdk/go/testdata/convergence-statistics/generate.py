"""Generate independently checked pinned QuantLib diagnostic fixtures."""

import argparse
import hashlib
import json
import math
from fractions import Fraction
from pathlib import Path
import subprocess

COMMIT = "9863b578af0caa4cecabf697196533e84a8308b6"
SOURCES = [
    "ql/math/statistics/convergencestatistics.hpp",
    "ql/math/statistics/generalstatistics.hpp",
    "ql/math/statistics/generalstatistics.cpp",
    "ql/errors.cpp",
]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n")


def setup():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quantlib", required=True, type=Path)
    parser.add_argument(
        "--boost-include", type=Path, default=Path("/opt/homebrew/include")
    )
    parser.add_argument("--cxx", default="clang++")
    parser.add_argument("--output-dir", type=Path, default=Path(__file__).parent)
    parser.add_argument("--build-dir", required=True, type=Path)
    args = parser.parse_args()
    root = args.quantlib.resolve()
    commit = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if commit != COMMIT:
        raise ValueError(f"Expected QuantLib {COMMIT}, got {commit}")
    hashes = {}
    for relative in SOURCES:
        path = root / relative
        pinned = subprocess.check_output(
            ["git", "-C", str(root), "show", f"{COMMIT}:{relative}"]
        )
        if path.read_bytes() != pinned:
            raise ValueError(f"Modified QuantLib source: {relative}")
        hashes[relative] = digest(path)
    native_source = Path(__file__).with_name("native.cpp").resolve()
    compiler = subprocess.check_output([args.cxx, "--version"], text=True).splitlines()[
        0
    ]
    args.build_dir.mkdir(parents=True, exist_ok=True)
    binary = args.build_dir / "convergence-statistics-native"
    command = [
        args.cxx,
        "-std=c++17",
        "-O2",
        "-I",
        str(root),
        "-I",
        str(args.boost_include),
        str(native_source),
    ]
    command += [str(root / path) for path in SOURCES if path.endswith(".cpp")]
    subprocess.run(command + ["-o", str(binary)], check=True)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    provenance = dict(
        source_commit=COMMIT,
        source_sha256=hashes,
        compiler=compiler,
        compiler_flags=["-std=c++17", "-O2"],
        generator_sha256=digest(Path(__file__)),
        native_source_sha256=digest(native_source),
    )
    return args, binary, provenance


def native(binary, data):
    return json.loads(subprocess.check_output([str(binary)], input=data, text=True))


CASES = [
    ("unit_fifteen", list(range(1, 16)), None),
    ("unequal_weights", [2, 4, -1, 8, 3, 10, 7], [1, 3, 2, 1, 4, 2, 5]),
    ("zero_weight_count", [2, 100, 8, -99, 4, 17, 6], [1, 0, 3, 0, 2, 0, 1]),
    ("constant", [7] * 15, [2] * 15),
    ("incomplete_checkpoint", [2, 6, 10, 20, 30], None),
    ("empty", [], None),
]


def exact(observations, weights):
    total = Fraction(0)
    weighted = Fraction(0)
    table = []
    checkpoint = 1
    for count, (value, weight) in enumerate(zip(observations, weights), 1):
        total += Fraction(weight)
        weighted += Fraction(value) * Fraction(weight)
        if count == checkpoint:
            table.append(dict(samples=count, mean=float(weighted / total)))
            checkpoint = 2 * checkpoint + 1
    return dict(
        samples=len(observations),
        weight_sum=float(total),
        mean=float(weighted / total) if observations else None,
        table=table,
    )


def main():
    args, binary, provenance = setup()
    files = []
    for name, observations, optional_weights in CASES:
        weights = (
            optional_weights
            if optional_weights is not None
            else [1] * len(observations)
        )
        data = (
            str(len(observations))
            + "\n"
            + "\n".join(
                f"{value} {weight}" for value, weight in zip(observations, weights)
            )
        )
        actual = native(binary, data)
        reference = exact(observations, weights)
        if (
            actual["samples"] != reference["samples"]
            or actual["weight_sum"] != reference["weight_sum"]
        ):
            raise ValueError(name)
        if observations and not math.isclose(
            actual["mean"], reference["mean"], rel_tol=2e-12, abs_tol=2e-12
        ):
            raise ValueError(name)
        if len(actual["table"]) != len(reference["table"]):
            raise ValueError(name)
        for point, expected in zip(actual["table"], reference["table"]):
            if point["samples"] != expected["samples"] or not math.isclose(
                point["mean"], expected["mean"], rel_tol=2e-12, abs_tol=2e-12
            ):
                raise ValueError(name)
        filename = f"{name}.json"
        files.append(filename)
        write(
            args.output_dir / filename,
            dict(
                name=name,
                observations=observations,
                weights=optional_weights,
                native=actual,
                exact=reference,
            ),
        )
    divergence = json.loads(
        subprocess.check_output([str(binary), "zero-first"], text=True)
    )
    if divergence != dict(
        rejected=False,
        samples_after_error=1,
        table_size_after_error=1,
        checkpoint_mean_is_nan=True,
    ):
        raise ValueError(f"Unexpected native failed-add semantics: {divergence}")
    provenance["native_reference"] = "Actual ConvergenceStatistics<GeneralStatistics>"
    provenance["exact_reference"] = "Independent Fraction weighted prefix means"
    write(
        args.output_dir / "oracle.json",
        dict(
            provenance=provenance,
            cases=files,
            tolerance=dict(relative=2e-12, absolute=2e-12),
            checkpoint_rule="initial=1; next=2*current+1",
            native_zero_first=divergence,
        ),
    )


if __name__ == "__main__":
    main()
