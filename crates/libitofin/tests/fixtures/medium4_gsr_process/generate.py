"""Independent Decimal evaluation of the original QuantLib moment integrals."""

from decimal import Decimal as D, localcontext
from pathlib import Path

CASES = [
    ("ordinary", ".03", ".012", "30", "2.5", ".007", "1.75"),
    ("negative", "-.03", ".012", "30", "2.5", "-.007", "1.75"),
    ("zero", "0", ".012", "30", "2.5", ".007", "1.75"),
    ("tiny_positive", "1e-14", ".012", "30", "2.5", ".007", "1.75"),
    ("tiny_negative", "-1e-14", ".012", "30", "2.5", "-.007", "1.75"),
    ("below_native_cutoff", ".00001", ".012", "30", "2.5", ".007", "1.75"),
    ("negative_below_native_cutoff", "-.00001", ".012", "30", "2.5", "-.007", "1.75"),
    ("large_reversion", "2", ".012", "30", "2.5", ".007", "1.75"),
    ("negative_reversion", "-2", ".012", "4.25", "2.5", ".007", "1.75"),
    ("terminal", ".03", ".012", "4.25", "2.5", ".007", "1.75"),
    ("origin_terminal", "-.03", ".012", "4.25", "0", "0", "4.25"),
    ("zero_interval", ".03", ".012", "30", "2.5", ".007", "0"),
    ("zero_vol", "-.03", "0", "30", "2.5", ".007", "1.75"),
]


def values(a, sigma, horizon, w, x, dt):
    t = w + dt
    s2 = sigma * sigma
    if a == 0:
        g = horizon - w
        y = s2 * w
        rn = s2 * (t * t - w * w) / 2
        tf = -s2 * dt * (horizon - (w + t) / 2)
        mean, variance = x + rn + tf, s2 * dt
    else:
        g = (1 - (-a * (horizon - w)).exp()) / a
        y = s2 * (1 - (-2 * a * w).exp()) / (2 * a)
        rn = (
            s2
            / (2 * a * a)
            * ((-2 * a * t).exp() + 1 - (-a * (w + t)).exp() - (-a * (t - w)).exp())
        )
        tf = (
            -s2
            / (2 * a * a)
            * (
                2
                - (a * (t - horizon)).exp()
                - 2 * (-a * (t - w)).exp()
                + (a * (2 * w - horizon - t)).exp()
            )
        )
        mean = x * (-a * dt).exp() + rn + tf
        variance = s2 * (1 - (-2 * a * dt).exp()) / (2 * a)
    return y - s2 * g - a * x, g, y, mean, variance


if __name__ == "__main__":
    lines = ["name\ta\tsigma\tT\tw\tx\tdt\tdrift\tg\ty\tmean\tvariance"]
    with localcontext() as ctx:
        ctx.prec = 110
        for row in CASES:
            result = values(*map(D, row[1:]))
            lines.append("\t".join((*row, *(str(float(v)) for v in result))))
    Path(__file__).with_name("reference.tsv").write_text("\n".join(lines) + "\n")
