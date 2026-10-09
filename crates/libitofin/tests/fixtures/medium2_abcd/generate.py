"""Print reproducible Rust arrays from compiled QuantLib 1.43 exports."""
import QuantLib as q

assert q.__version__ == "1.43"
sets = [(0.002, 0.001, 0.16, 0.0005), (-0.06, 0.17, 0.54, 0.17),
        (0.2, -0.1, 0.5, 0.2), (-0.1, 0.0, 0.6, 0.1), (0.0, 0.0, 0.7, 0.2)]

def rust(values):
    return "[" + ", ".join(repr(float(v)).replace("e-0", "e-").replace("e+0", "e+") for v in values) + "]"

print("const NATIVE_VALUES: &[[f64; 6]] = &[")
for coeff in sets:
    f = q.AbcdMathFunction(*coeff)
    for t in [-2, 0, 0.1, 1, 5, 25]:
        print("    " + rust([*coeff, t, f(t)]) + ",")
print("];\nconst NATIVE_COVARIANCES: &[[f64; 9]] = &[")
for coeff in sets:
    f = q.AbcdFunction(*coeff)
    for lo, hi, t, s in [(0, 1, 2, 3), (0.5, 2.5, 2, 3), (-1, 0.5, 1, 2),
                         (1, 1, 2, 3), (3, 5, 2, 3), (0, 3, 2, 2)]:
        print("    " + rust([*coeff, lo, hi, t, s, f.covariance(lo, hi, t, s)]) + ",")
print("];\nconst NATIVE_INSTANTANEOUS: &[[f64; 8]] = &[")
for coeff in sets:
    f = q.AbcdFunction(*coeff)
    for u, t, s in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (-1, 1, 2)]:
        print("    " + rust([*coeff, u, t, s, f.instantaneousCovariance(u, t, s)]) + ",")
print("];")

from decimal import Decimal as D, localcontext

print("\nconst HIGH_PRECISION_SMALL_DECAY: &[[f64; 2]] = &[")
with localcontext() as context:
    context.prec = 80
    for text in ["1e-50", "1e-15", "1e-8"]:
        c, a, b, d = D(text), D(".2"), D(".1"), D(".3")
        at, ass = a + b, a + 2*b
        et, es = (-c).exp(), (-2*c).exp()

        def moment(rate, n):
            term = D(1)
            total = term / D(n + 1)
            for k in range(1, 100):
                term *= -rate / D(k)
                total += term / D(n + k + 1)
            return total

        result = (et*es*(at*ass*moment(2*c, 0) + b*(at+ass)*moment(2*c, 1)
                         + b*b*moment(2*c, 2))
                  + d*(et*(at*moment(c, 0)+b*moment(c, 1))
                       + es*(ass*moment(c, 0)+b*moment(c, 1))) + d*d)
        print("    " + rust([c, result]) + ",")
print("];")
