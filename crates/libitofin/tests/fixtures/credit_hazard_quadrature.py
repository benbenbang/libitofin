"""Print hazardratestructure.rs oracle tuples using QuantLib 1.43.

Run: uv run --with QuantLib==1.43 credit_hazard_quadrature.py
The remapper and Jacobian follow hazardratestructure.cpp:77-82.
"""

import math

import QuantLib as ql

assert ql.__version__ == "1.43"
rule = ql.GaussChebyshevIntegration(48)


def survival(t, linear, quadratic):
    def remapped(x):
        tau = (x + 1.0) * t / 2.0
        return 0.04 + linear * tau + quadratic * tau * tau

    return math.exp(-rule(remapped) * t / 2.0)


for t in [0.0, 0.25, 1.0, 2.5, 5.0]:
    print(f"({t!r}, {survival(t, 0.0, 0.0)!r}, {survival(t, 0.01, 0.002)!r}),")
