"""Print the standard normal references for normal.rs's testCumulativeNormal port.

Run: uv run --with mpmath==1.3.0 normal_cdf_v144.py
QuantLib v1.44 distributions.cpp:466-486 compares against Boost.Math's normal
quantile and cdf; these are the same quantities at 50 significant digits,
rounded once to the nearest double. The probes around the switch are formed in
double arithmetic, exactly as the Rust test forms them from the switch value.
"""

import mpmath

mpmath.mp.dps = 50


def cdf(x):
    return float(mpmath.ncdf(mpmath.mpf(x)))


switch = float(-mpmath.sqrt(2) * mpmath.erfinv(1 - 2 * mpmath.mpf(1.0e-8)))
print(f"switch = {switch!r}")
for x in [switch - 1.0e-6, switch, switch + 1.0e-6, -6.0, -10.0, -20.0, 6.0, 10.0, 20.0]:
    print(f"({x!r}, {cdf(x)!r}),")
