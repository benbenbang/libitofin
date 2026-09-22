"""Regenerate the Heston oracle using exported QuantLib 1.43 wheel symbols."""

import argparse
from collections import Counter
import ctypes
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("quantlib", type=Path)
    parser.add_argument("--boost-include", type=Path, default=Path("/opt/homebrew/include"))
    parser.add_argument("--load", type=Path)
    args = parser.parse_args()
    import QuantLib as ql
    import QuantLib._QuantLib as native

    if ql.__version__ != "1.43" or sys.platform != "darwin":
        raise SystemExit("requires macOS and the independent QuantLib 1.43 wheel")
    if args.load:
        ctypes.CDLL(native.__file__, mode=ctypes.RTLD_GLOBAL)
        ctypes.CDLL(str(args.load)).main()
        return
    folder = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix="heston-oracle-") as temporary:
        bundle = Path(temporary) / "oracle.so"
        subprocess.run([
            os.environ.get("CXX", "clang++"), "-std=c++17", "-O2", "-bundle",
            "-undefined", "dynamic_lookup", "-I", str(args.boost_include),
            "-I", str(args.quantlib), str(folder / "oracle.cpp"), "-o", str(bundle),
        ], check=True)
        result = subprocess.run([
            sys.executable, str(Path(__file__).resolve()), str(args.quantlib),
            "--load", str(bundle),
        ], check=True, stdout=subprocess.PIPE)
        counts = Counter(row.split(b",", 1)[0] for row in result.stdout.splitlines())
        if counts != {b"c": 13, b"f": 78, b"p": 42}:
            raise ValueError(f"unexpected oracle row counts: {counts}")
        candidate = folder / "oracle.csv.tmp"
        candidate.write_bytes(result.stdout)
        candidate.replace(folder / "oracle.csv")


if __name__ == "__main__":
    main()
