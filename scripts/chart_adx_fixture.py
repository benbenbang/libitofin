#!/usr/bin/env python3
"""Independent exact-rational ADX fixture, with every seed and recurrence step.

Print CSV to stdout. Uses only standard-library Fraction arithmetic, no itofin
or external indicator implementation. Period two excludes bar zero from seeds.
"""

# standard library
import csv
import sys
from fractions import Fraction as F

HIGH = [10, 12, 11, 14, 15, 13, 18, 16]
LOW = [8, 9, 7, 10, 9, 9, 15, 13]
CLOSE = [9, 11, 8, 13, 10, 12, 17, 14]


def main():
    """Emit deterministic raw movements, smoothed means and indicator values."""
    writer = csv.writer(sys.stdout, lineterminator="\n")
    writer.writerow(
        [
            "high",
            "low",
            "close",
            "tr",
            "plus_dm",
            "minus_dm",
            "mean_tr",
            "mean_plus",
            "mean_minus",
            "plus_di",
            "minus_di",
            "dx",
            "adx",
        ]
    )
    previous = None
    previous_adx = None
    first_dx = None
    seed = [F(0)] * 3
    for i, (h, low, c) in enumerate(zip(HIGH, LOW, CLOSE)):
        tr = (
            F(h - low)
            if i == 0
            else F(max(h - low, abs(h - CLOSE[i - 1]), abs(low - CLOSE[i - 1])))
        )
        up = h - HIGH[i - 1] if i else 0
        down = LOW[i - 1] - low if i else 0
        plus = F(up if up > 0 and up > down else 0)
        minus = F(down if down > 0 and down > up else 0)
        row = [h, low, c, tr, plus, minus]
        if i in (1, 2):
            seed = [s + v for s, v in zip(seed, [tr, plus, minus])]
        if i >= 2:
            previous = (
                [s / 2 for s in seed]
                if i == 2
                else [(s + v) / 2 for s, v in zip(previous, [tr, plus, minus])]
            )
            mean_tr, mean_plus, mean_minus = previous
            pdi = 100 * mean_plus / mean_tr if mean_tr else F(0)
            mdi = 100 * mean_minus / mean_tr if mean_tr else F(0)
            dx = 100 * abs(pdi - mdi) / (pdi + mdi) if pdi + mdi else F(0)
            if i == 2:
                first_dx = dx
            if i >= 3:
                previous_adx = (
                    (first_dx + dx) / 2 if i == 3 else (previous_adx + dx) / 2
                )
            row += [*previous, pdi, mdi, dx, previous_adx if i >= 3 else 0]
        else:
            row += [0] * 7
        writer.writerow(row)


if __name__ == "__main__":
    main()
