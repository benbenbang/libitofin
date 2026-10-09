"""Compile the pinned original QuantLib classes and print Rust fixture tuples."""
import argparse
import pathlib
import subprocess
import tempfile

PIN = "9863b578af0caa4cecabf697196533e84a8308b6"
CASES = [
    ([1, 0, 0], [3, 4, 0]),
    ([1, 0, 0], [-3, 4, 0]),
    ([1, 0, 0], [0, 4, 3]),
    ([1, 0, 0], [3, 0, 0]),
    ([1, 0, 0], [-3, 0, 0]),
    ([1, 0, 0], [1, 1e-3, -2e-3]),
    ([1, 0, 0], [-1, 1e-3, -2e-3]),
    ([1, 0, 0], [1, 1e-12, 0]),
    ([1, 0, 0], [-1, 1e-12, 0]),
    ([0.6, 0.8, 0], [1, 2, 3]),
    ([0, 0, -1], [0.1, 0.2, 0.3]),
]
CPP = r"""
#include <ql/math/matrixutilities/householder.hpp>
#include <iomanip>
#include <iostream>
int main() {
    std::size_t n;
    while (std::cin >> n) {
        QuantLib::Array e(n), a(n);
        for (auto& x : e) std::cin >> x;
        for (auto& x : a) std::cin >> x;
        QuantLib::HouseholderReflection r(e);
        const auto v = r.reflectionVector(a);
        const auto y = r(a);
        std::cout << std::setprecision(17);
        for (auto x : v) std::cout << x << ' ';
        for (auto x : y) std::cout << x << ' ';
        std::cout << '\n';
    }
}
"""


def array(values):
    return "[" + ", ".join(repr(float(x)) for x in values) + "]"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quantlib", type=pathlib.Path, required=True)
    parser.add_argument("--boost-include", default="/opt/homebrew/include")
    parser.add_argument("--compiler", default="c++")
    args = parser.parse_args()
    pin = subprocess.check_output(["git", "-C", str(args.quantlib), "rev-parse", "HEAD"], text=True).strip()
    if pin != PIN:
        raise SystemExit(f"Expected source {PIN}, found {pin}")
    with tempfile.TemporaryDirectory(prefix="householder-oracle-") as directory:
        directory = pathlib.Path(directory)
        source = directory / "probe.cpp"
        binary = directory / "probe"
        source.write_text(CPP)
        subprocess.run([
            args.compiler, "-std=c++17", "-O2", "-I" + str(args.quantlib),
            "-I" + args.boost_include, str(source),
            str(args.quantlib / "ql/math/matrixutilities/householder.cpp"),
            str(args.quantlib / "ql/errors.cpp"), "-o", str(binary),
        ], check=True)
        inputs = "".join(f"{len(e)} " + " ".join(map(str, e + a)) + "\n" for e, a in CASES)
        output = subprocess.check_output([str(binary)], input=inputs, text=True)
        print("type Oracle = ([f64; 3], [f64; 3], [f64; 3], [f64; 3]);\n\nconst ORACLES: &[Oracle] = &[")
        lines = output.splitlines()
        if len(lines) != len(CASES):
            raise RuntimeError("Original probe returned an unexpected case count")
        for (e, a), line in zip(CASES, lines):
            values = list(map(float, line.split()))
            print(f"    ({array(e)}, {array(a)}, {array(values[:3])}, {array(values[3:])}),")
        print("];")


if __name__ == "__main__":
    main()
