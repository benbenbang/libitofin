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
    "ql/math/statistics/discrepancystatistics.hpp",
    "ql/math/statistics/discrepancystatistics.cpp",
    "ql/math/statistics/sequencestatistics.hpp",
    "ql/math/statistics/generalstatistics.hpp",
    "ql/math/statistics/generalstatistics.cpp",
    "ql/math/statistics/statistics.hpp",
    "ql/math/randomnumbers/sobolrsg.hpp",
    "ql/math/randomnumbers/sobolrsg.cpp",
    "ql/math/randomnumbers/primitivepolynomials.hpp",
    "ql/math/randomnumbers/primitivepolynomials.cpp",
    "ql/math/randomnumbers/mt19937uniformrng.hpp",
    "ql/math/randomnumbers/mt19937uniformrng.cpp",
    "ql/math/randomnumbers/seedgenerator.hpp",
    "ql/math/randomnumbers/seedgenerator.cpp",
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
    binary = args.build_dir / "discrepancy-statistics-native"
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
    ("boundary_corners", [[0, 0], [0, 1], [1, 0], [1, 1]], None),
    ("duplicates", [[0.5, 0.5]] * 3, [1, 1, 1]),
    ("interior", [[0.25, 0.25], [0.75, 0.75], [0.25, 0.75], [0.75, 0.25]], None),
    ("one_point_3d", [[0.25, 0.5, 0.75]], None),
]
GENERATED = [("sobol", 2, 8), ("sobol", 3, 16), ("mt19937", 2, 8), ("mt19937", 3, 16)]
SEED = 1729


def exact_squared(samples):
    points = [[Fraction(x) for x in row] for row in samples]
    count, dimension = len(points), len(points[0])
    a = sum(
        math.prod(1 - max(x, y) for x, y in zip(left, right))
        for left in points
        for right in points
    )
    c = sum(math.prod(1 - coordinate**2 for coordinate in point) for point in points)
    return (
        a / count**2
        - Fraction(1, 2 ** (dimension - 1)) * c / count
        + Fraction(1, 3**dimension)
    )


def main():
    args, binary, provenance = setup()
    evaluated = []
    for name, samples, weights in CASES:
        header = f"input {len(samples[0])} {len(samples)} {SEED}\n"
        actual = native(
            binary, header + "\n".join(" ".join(map(str, row)) for row in samples)
        )
        if actual["samples"] != samples:
            raise ValueError(name)
        evaluated.append((name, actual, weights, dict(kind="hand-authored")))
    for kind, dimension, count in GENERATED:
        actual = native(binary, f"{kind} {dimension} {count} {SEED}\n")
        evaluated.append(
            (
                f"{kind}_{dimension}d_{count}",
                actual,
                None,
                dict(kind=kind, seed=SEED, direction_integers="Jaeckel")
                if kind == "sobol"
                else dict(kind=kind, seed=SEED),
            )
        )
    files = []
    for name, actual, weights, source in evaluated:
        squared = exact_squared(actual["samples"])
        if squared < 0:
            raise ValueError(name)
        reference = math.sqrt(float(squared))
        if not math.isclose(
            actual["discrepancy"], reference, rel_tol=2e-12, abs_tol=2e-12
        ):
            raise ValueError(
                f"{name}: native {actual['discrepancy']} != exact {reference}"
            )
        filename = f"{name}.json"
        files.append(filename)
        write(
            args.output_dir / filename,
            dict(
                name=name,
                samples=actual["samples"],
                weights=weights,
                dimension=len(actual["samples"][0]),
                native_discrepancy=actual["discrepancy"],
                exact_discrepancy=reference,
                exact_squared=dict(
                    numerator=str(squared.numerator),
                    denominator=str(squared.denominator),
                ),
                point_source=source,
            ),
        )
    rejected = json.loads(
        subprocess.check_output([str(binary), "dimension-one"], text=True)
    )
    if rejected != dict(rejected=True):
        raise ValueError(rejected)
    provenance["native_reference"] = (
        "Actual QuantLib DiscrepancyStatistics; actual pinned SobolRsg/Jaeckel and MersenneTwisterUniformRng"
    )
    provenance["exact_reference"] = (
        "Fraction.from_float coordinates; independent ordered double sum; sqrt(float(exact squared))"
    )
    write(
        args.output_dir / "oracle.json",
        dict(
            provenance=provenance,
            cases=files,
            tolerance=dict(relative=2e-12, absolute=2e-12),
            native_dimension_one=rejected,
            formula="A/N^2 - 2^(1-d)*C/N + 3^(-d)",
        ),
    )


if __name__ == "__main__":
    main()
