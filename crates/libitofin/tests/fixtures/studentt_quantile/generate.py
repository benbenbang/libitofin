"""Student-t quantile oracle for libitofin #1418.

Each row is the exact quantile of the Student-t distribution with `nu`
degrees of freedom at the binary64 probability `p` (taken exactly, not the
decimal literal), computed with mpmath at 60 significant digits by bisection
on the regularized incomplete beta function and rounded to 17 digits.

Regenerate from the repository root:

    uv run --with mpmath==1.3.0 python -I \
        crates/libitofin/tests/fixtures/studentt_quantile/generate.py \
        > crates/libitofin/tests/fixtures/studentt_quantile/quantiles.csv
"""

import mpmath

mpmath.mp.dps = 60

NUS = [2.5, 3.0, 4.0, 5.0, 6.0, 10.0, 30.0]
PROBABILITIES = [
    1.0e-12, 1.0e-9, 1.0e-6, 1.0e-4, 0.001, 0.01, 0.1, 0.15, 0.2, 0.25,
    0.3, 0.4, 0.49, 0.6, 0.75, 0.9, 0.99, 0.999, 1.0 - 1.0e-4,
    1.0 - 1.0e-6, 1.0 - 1.0e-9,
]


def lower_cdf(nu, t):
    z = nu / (nu + t * t)
    return mpmath.betainc(nu / 2, mpmath.mpf(1) / 2, 0, z, regularized=True) / 2


def lower_quantile(nu, p):
    lo, hi = mpmath.mpf(-1e30), mpmath.mpf(0)
    for _ in range(400):
        mid = (lo + hi) / 2
        if lower_cdf(nu, mid) < p:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def quantile(nu, p):
    nu, p = mpmath.mpf(nu), mpmath.mpf(p)
    if p == mpmath.mpf(1) / 2:
        return mpmath.mpf(0)
    if p < mpmath.mpf(1) / 2:
        return lower_quantile(nu, p)
    return -lower_quantile(nu, 1 - p)


def main():
    print(f"# mpmath {mpmath.__version__}, dps {mpmath.mp.dps}; nu,p,quantile")
    for nu in NUS:
        for p in PROBABILITIES:
            x = quantile(nu, p)
            print(f"{nu!r},{p!r},{mpmath.nstr(x, 17, min_fixed=-1, max_fixed=1)}")


if __name__ == "__main__":
    main()
