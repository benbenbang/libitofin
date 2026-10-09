"""Compute Wilder trend strength with independently seeded warmup channels."""

# itofin library
from itofin import chart

result = chart.adx(
    high=[10, 12, 11, 14],
    low=[8, 9, 7, 10],
    close=[9, 11, 8, 13],
    period=2,
)
assert result.dx.first_valid == 2
assert result.adx.first_valid == 3
assert result.adx.to_list()[:3] == [None, None, None]
assert abs(result.adx.values[3] - 30) < 1e-12
print("ADX:", result.adx.to_list())
