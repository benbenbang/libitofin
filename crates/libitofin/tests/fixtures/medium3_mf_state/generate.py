"""Independent Decimal antiderivative oracle, not a Rust implementation wrapper."""
from decimal import Decimal as D, localcontext
from pathlib import Path

PIN = "9863b578af0caa4cecabf697196533e84a8308b6"
CASES = [
    ("empty_zero", "0", "-", "0.3", "0.7", "1.4"),
    ("empty_positive", "0.13", "-", "0", "0.7", "1.4"),
    ("empty_negative", "-0.13", "-", "4", "0.7", "1.4"),
    ("all_buckets", "0.17", "0.5;1;2", "0.2;0;0.4;0.1", "0.2", "2.3"),
    ("signed_buckets", "-0.17", "0.5;1;2", "0.2;0;0.4;0.1", "0.2", "2.3"),
    ("boundary_start", "0.17", "0.5;1;2", "0.2;0;0.4;0.1", "1", "1"),
    ("zero_reversion", "0", "0.5;1;2", "0.2;0;0.4;0.1", "0.2", "2.3"),
    ("near_zero", "1e-15", "0.5;1;2", "0.2;0;0.4;0.1", "0.2", "2.3"),
    ("tiny_interval", "0.2", "1;2", "0.3;0.2;0.4", "0.5", "1e-12"),
    ("zero_boundary", "-0.2", "0;1", "0.9;0.2;0.4", "0", "1.5"),
    ("tail", "-0.5", "1;2", "0.3;0.2;0.4", "10", "2"),
    ("subnormal_vol_square", "350", "3", "1e-160;1e-160", "0", "1"),
    ("subnormal_exponential", "-370", "3", "1e150;1e150", "1", "1"),
    ("scaled_overflow", "400", "2", "1e-170;0", "0", "1"),
    ("scaled_underflow", "-400", "2", "1e170;0", "1", "0.5"),
]


def reference(a, boundaries, sigmas, start, length):
    a, start, length = (D.from_float(float(value)) for value in (a, start, length))
    if length < D.from_float(float.fromhex("0x1p-52")):
        return D(0)
    if abs(a) < D.from_float(float.fromhex("0x1p-52")):
        a = D(0)
    times = [] if boundaries == "-" else [D.from_float(float(value)) for value in boundaries.split(";")]
    vols = [D.from_float(float(value)) for value in sigmas.split(";")]
    if not times:
        vols = [D(1)]
    if not times and a == 0:
        return length
    end = D.from_float(float(start) + float(length))
    cuts = [start] + [t for t in times if start < t < end] + [end]
    total = D(0)
    for left, right in zip(cuts, cuts[1:]):
        sigma = vols[sum(t <= left for t in times)]
        integral = right - left if a == 0 else (
            (2 * a * right).exp() - (2 * a * left).exp()
        ) / (2 * a)
        total += sigma * sigma * integral
    return total


with localcontext() as context:
    context.prec = 80
    rows = ["name\ta\ttimes\tvols\tt0\tdt\tvariance\tstddev"]
    for name, a, times, vols, t0, dt in CASES:
        variance = reference(a, times, vols, t0, dt)
        rows.append("\t".join((name, a, times, vols, t0, dt,
                               format(variance, ".55g"), format(variance.sqrt(), ".55g"))))
    Path(__file__).with_name("reference.tsv").write_text("\n".join(rows) + "\n")
print(f"generated {len(CASES)} source-law fixtures from pinned QuantLib {PIN}")

if __name__ == "__main__":
    import argparse
    import subprocess
    import tempfile

    parser = argparse.ArgumentParser()
    parser.add_argument("--quantlib", type=Path, help="compile the pinned original source too")
    parser.add_argument("--boost-include", default="/opt/homebrew/include")
    args = parser.parse_args()
    if args.quantlib:
        ql = args.quantlib.resolve()
        revision = subprocess.check_output(["git", "-C", str(ql), "rev-parse", "HEAD"], text=True).strip()
        if revision != PIN:
            raise SystemExit(f"expected {PIN}, found {revision}")
        here = Path(__file__).resolve().parent
        with tempfile.TemporaryDirectory(prefix="mf-state-native-") as temp:
            executable = Path(temp) / "native"
            sources = ["processes/mfstateprocess.cpp", "stochasticprocess.cpp",
                       "patterns/observable.cpp", "errors.cpp"]
            command = ["clang++", "-std=c++17", "-O2", f"-I{ql}",
                       f"-I{args.boost_include}", str(here / "native.cpp")]
            command += [str(ql / "ql" / source) for source in sources]
            subprocess.run(command + ["-o", str(executable)], check=True)
            output = subprocess.check_output([str(executable)], text=True)
            (here / "native.tsv").write_text(output)
        print("generated 51 fixtures by compiling the actual pinned original class")
