"""Regenerate BMA fixtures using a QuantLib 1.43 wheel on macOS."""

import argparse
import csv
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import QuantLib
import QuantLib._QuantLib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    fixture = Path(__file__).resolve().parent
    root = fixture.parents[4]
    parser.add_argument("--quantlib-include", type=Path, default=root / "QuantLib")
    parser.add_argument("--boost-include", type=Path, default=Path("/opt/homebrew/include"))
    args = parser.parse_args()
    if sys.platform != "darwin" or QuantLib.__version__ != "1.43":
        raise RuntimeError("requires macOS and the independent QuantLib 1.43 wheel")
    with tempfile.TemporaryDirectory() as temporary:
        bundle = Path(temporary) / "bma.so"
        subprocess.run(
            [os.environ.get("CXX", "clang++"), "-std=c++17", "-bundle",
             "-undefined", "dynamic_lookup", f"-I{args.quantlib_include}",
             f"-I{args.boost_include}", str(fixture / "oracle.cpp"), "-o", str(bundle)],
            check=True,
        )
        program = (
            "import ctypes, QuantLib._QuantLib as q; "
            "ctypes.CDLL(q.__file__, mode=ctypes.RTLD_GLOBAL); "
            "ctypes.CDLL(__import__('sys').argv[1]).bma_oracle()"
        )
        data = json.loads(subprocess.check_output([sys.executable, "-c", program, str(bundle)]))
    data["headers"] = data["quantlib"]
    data["quantlib"] = QuantLib.__version__
    encoded = json.dumps(data, indent=2, sort_keys=True) + "\n"
    (fixture / "oracle.json").write_text(encoded)
    (root / "sdk/go/testdata/bma.json").write_text(encoded)
    output = io.StringIO()
    writer = csv.DictWriter(output, fieldnames=list(data["nodes"][0]), lineterminator="\n")
    writer.writeheader()
    writer.writerows(data["nodes"])
    (fixture / "swaps.csv").write_text(output.getvalue())


if __name__ == "__main__":
    main()
