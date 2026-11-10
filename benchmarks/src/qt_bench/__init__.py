# standard library
import platform
from importlib.metadata import version

# qt bench library
from qt_bench.main import STRIKES, bench, itofin_european_sweep, itofin_heston, ql_european_sweep, ql_heston


def main():
    print(
        f"Python {platform.python_version()} | {platform.machine()} | "
        f"QuantLib {version('QuantLib')} | itofin {version('itofin')}\n"
    )

    # Correctness first
    it_eu, ql_eu = itofin_european_sweep(), ql_european_sweep()
    max_npv = max(abs(a[0] - b[0]) for a, b in zip(it_eu, ql_eu))
    max_dlt = max(abs(a[1] - b[1]) for a, b in zip(it_eu, ql_eu))
    print("== Correctness ==")
    print(f"European K=65 NPV: itofin {it_eu[500][0]:.10f} | QuantLib {ql_eu[500][0]:.10f}")
    print(f"Max |diff| over {len(STRIKES)} strikes: NPV {max_npv:.2e}, delta {max_dlt:.2e}")
    it_h, ql_h = itofin_heston(), ql_heston()
    for name, a, b in zip(("sigma", "v0", "theta"), it_h, ql_h):
        print(f"Heston {name:<5}: itofin {a:.6f} | QuantLib {b:.6f}")

    # Speed
    print("\n== Speed (median of 7 runs, lower is better) ==")
    t_it = bench(itofin_european_sweep, number=5)
    t_ql = bench(ql_european_sweep, number=5)
    n = len(STRIKES)
    print(
        f"European, per option : itofin {t_it / n * 1e6:8.2f} us | "
        f"QuantLib {t_ql / n * 1e6:8.2f} us | ratio QL/itofin {t_ql / t_it:5.2f}x"
    )

    t_it = bench(itofin_heston, number=3)
    t_ql = bench(ql_heston, number=3)
    print(
        f"Heston calibration   : itofin {t_it * 1e3:8.2f} ms | "
        f"QuantLib {t_ql * 1e3:8.2f} ms | ratio QL/itofin {t_ql / t_it:5.2f}x"
    )
    print("\nratio > 1 means itofin is faster.")


if __name__ == "__main__":
    main()
