"""Reproduce the source-qualified CPI cap/floor native price fixture."""

import argparse
import csv
import ctypes
import hashlib
import json
import math
import os
import subprocess
from pathlib import Path

import QuantLib as q
import QuantLib._QuantLib as native


def sha(path):
    """Hash a recorded oracle input or output."""
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    """Compile the wheel-linked native probe and record its exact inputs."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--quantlib-source", required=True, type=Path)
    parser.add_argument("--boost-include", required=True, type=Path)
    parser.add_argument("--build-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--sdkroot", type=Path)
    parser.add_argument("--compiler", default="clang++")
    args = parser.parse_args()
    if os.uname().sysname != "Darwin":
        raise RuntimeError(
            "this wheel-linked oracle runner currently supports macOS only"
        )
    args.build_dir.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).with_suffix(".cpp")
    library = args.build_dir / "cpi_capfloor_oracle.dylib"
    dependencies = args.build_dir / "cpi_capfloor_oracle.d"
    command = [
        args.compiler,
        "-std=c++17",
        "-dynamiclib",
        "-undefined",
        "dynamic_lookup",
        "-MMD",
        "-MF",
        str(dependencies),
        "-I" + str(args.quantlib_source),
        "-I" + str(args.boost_include),
        str(source),
        "-o",
        str(library),
    ]
    env = os.environ.copy()
    if args.sdkroot:
        env["SDKROOT"] = str(args.sdkroot)
    result = subprocess.run(
        command, check=False, env=env, text=True, capture_output=True
    )
    (args.build_dir / "compile.log").write_text(result.stdout + result.stderr)
    result.check_returncode()
    ctypes.CDLL(native.__file__, mode=os.RTLD_GLOBAL)
    harness = ctypes.CDLL(str(library.resolve()), mode=os.RTLD_GLOBAL)
    harness.run_oracle.argtypes = [ctypes.c_char_p]
    harness.run_oracle.restype = ctypes.c_int
    status = harness.run_oracle(os.fsencode(args.output.resolve()))
    if status:
        raise RuntimeError(f"native oracle failed with status {status}")
    with args.output.open() as stream:
        rows = {
            row["name"]: float(row["value"])
            for row in csv.DictReader(stream, delimiter="\t")
        }
    expected = {
        "sanity_discount": 0.96078943915232318,
        "sanity_index_forecast": 101.43447448717296,
        "sanity_coupon_amount": 999.71506849315074,
    }
    for name, value in expected.items():
        if not math.isclose(rows[name], value, rel_tol=1e-13, abs_tol=1e-12):
            raise AssertionError(f"ABI sanity mismatch {name}: {rows[name]} != {value}")
    shared_settings = q.Settings.instance().evaluationDate.ISO()
    if shared_settings != "2027-06-16":
        raise AssertionError(f"wheel/header singleton mismatch: {shared_settings}")
    revision = subprocess.check_output(
        ["git", "-C", str(args.quantlib_source), "rev-parse", "HEAD"], text=True
    ).strip()
    header_hashes = {}
    for token in dependencies.read_text().replace("\\\n", " ").split():
        path = Path(token)
        if path.is_file() and path.is_relative_to(args.quantlib_source):
            header_hashes[str(path.relative_to(args.quantlib_source))] = sha(path)
    manifest = {
        "schema": 1,
        "oracle_kind": "mixed_pinned_headers_and_native_wheel",
        "wheel_version": q.__version__,
        "wheel_native_sha256": sha(native.__file__),
        "header_source_revision": revision,
        "source_is_wheel_build_provenance": False,
        "template_code": "bilinear price-surface template compiled from the recorded source headers",
        "native_code": "CPICapFloor, InterpolatingCPICapFloorEngine, base-surface and underlying curve/index symbols from wheel",
        "abi_qualification": "three numerical sanity checks and shared Settings agree with SWIG; not proof of universal ABI compatibility",
        "compiler": subprocess.check_output(
            [args.compiler, "--version"], text=True
        ).splitlines()[0],
        "sdk": args.sdkroot.name if args.sdkroot else None,
        "generator_cpp_sha256": sha(source),
        "generator_python_sha256": sha(__file__),
        "harness_sha256": sha(library),
        "fixture_sha256": sha(args.output),
        "row_count": len(rows),
        "sanity_checks": expected,
        "shared_settings_after_run": shared_settings,
        "included_quantlib_headers": header_hashes,
    }
    args.output.with_suffix(".provenance.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    print(args.output.name, manifest["fixture_sha256"], len(rows))


if __name__ == "__main__":
    main()
