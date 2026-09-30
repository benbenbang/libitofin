"""Regenerate the seeded GARCH fit oracle with independent QuantLib 1.43."""

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile

import QuantLib
import QuantLib._QuantLib


COUNT = 50_000
UPSTREAM_EXPECTED = (0.207592, 0.281979, 0.204647, -0.0217413)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    fixture = Path(__file__).resolve().parent
    root = fixture.parents[4]
    parser.add_argument("--quantlib-include", type=Path, default=root / "QuantLib")
    parser.add_argument("--boost-include", type=Path, default=Path("/opt/homebrew/include"))
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if sys.platform != "darwin" or QuantLib.__version__ != "1.43":
        raise RuntimeError("requires macOS and the independent QuantLib 1.43 wheel")

    with tempfile.TemporaryDirectory() as temporary:
        bundle = Path(temporary) / "garch_fit.so"
        subprocess.run(
            [os.environ.get("CXX", "clang++"), "-std=c++17", "-bundle",
             "-undefined", "dynamic_lookup", f"-I{args.quantlib_include}",
             f"-I{args.boost_include}", str(fixture / "oracle.cpp"), "-o", str(bundle)],
            check=True,
        )
        ctypes.CDLL(QuantLib._QuantLib.__file__, mode=ctypes.RTLD_GLOBAL)
        library = ctypes.CDLL(str(bundle))
        library.garch_fit_oracle.argtypes = (ctypes.POINTER(ctypes.c_double),
                                             ctypes.POINTER(ctypes.c_double))
        library.garch_fit_oracle.restype = ctypes.c_int
        returns = (ctypes.c_double * COUNT)()
        result = (ctypes.c_double * 5)()
        if library.garch_fit_oracle(returns, result) != 0:
            raise RuntimeError("QuantLib GARCH oracle failed")

    if any(abs(actual - expected) > 1e-6
           for actual, expected in zip(result[:4], UPSTREAM_EXPECTED)):
        raise RuntimeError("QuantLib result differs from test-suite/garch.cpp")

    returns_bytes = struct.pack(f"<{COUNT}d", *returns)
    source_commit = subprocess.check_output(
        ["git", "-C", str(args.quantlib_include), "rev-parse", "HEAD"], text=True
    ).strip()
    version_header = (args.quantlib_include / "ql/version.hpp").read_text()
    header_version = re.search(r'#define QL_VERSION "([^"]+)"', version_header)
    if header_version is None:
        raise RuntimeError("cannot identify QuantLib header version")

    data = {
        "source": {
            "quantlib_wheel": QuantLib.__version__,
            "quantlib_headers": header_version.group(1),
            "quantlib_source_commit": source_commit,
            "upstream_test": "test-suite/garch.cpp::testCalibration",
        },
        "input": {
            "count": COUNT,
            "start_date": "1962-07-07",
            "seed": 48,
            "process_alpha": 0.2,
            "process_beta": 0.3,
            "process_long_term_variance": 0.4,
            "initial_return": 0.0,
            "initial_variance": 0.0,
            "returns_sha256_f64_le": hashlib.sha256(returns_bytes).hexdigest(),
            "first_returns": list(returns[:5]),
            "last_returns": list(returns[-5:]),
        },
        "expected": {
            "alpha": result[0],
            "beta": result[1],
            "omega": result[2],
            "log_likelihood": result[3],
            "next_variance": result[4],
        },
    }
    expected_json = (json.dumps(data, indent=2, sort_keys=True) + "\n").encode()
    if args.check:
        if (fixture / "returns.bin").read_bytes() != returns_bytes:
            raise RuntimeError("tracked returns.bin differs from QuantLib output")
        if (fixture / "oracle.json").read_bytes() != expected_json:
            raise RuntimeError("tracked oracle.json differs from QuantLib output")
    else:
        (fixture / "returns.bin").write_bytes(returns_bytes)
        (fixture / "oracle.json").write_bytes(expected_json)


if __name__ == "__main__":
    main()
