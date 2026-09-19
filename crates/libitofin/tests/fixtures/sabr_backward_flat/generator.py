"""QuantLib 1.43 SABR swaption cube oracle for the backwardFlat flag (#606).

Rebuilds the CommonVars cube of test-suite/swaptionvolatilitycube.cpp exactly as
crates/libitofin/.../sabrvolcube/sabrcube.rs::build_common_sabr_cube does
(15 June 2026, flat 5% Actual360 continuous curve, the moving 6x4 ATM matrix,
the 3x3 cube with five strike spreads, guess [0.2, 0.5, 0.4, 0.0], all
parameters free, C++ constructor defaults) and records the cube volatility at
option tenors between the parameter-cube nodes ({1Y, 10Y, 30Y} sparse;
{1M, 6M, 1Y, 5Y, 10Y, 30Y} once ATM-calibrated, so 5Y is a node there and the
two flags must agree), once with backwardFlat=False and once with
backwardFlat=True. Run from the repository root with
`uv run --with QuantLib==1.43 <this file>`.
"""

import csv
from pathlib import Path

import QuantLib as ql

assert ql.__version__ == "1.43", ql.__version__
ql.Settings.instance().evaluationDate = today = ql.Date(15, 6, 2026)
TARGET = ql.TARGET()
BDC = ql.ModifiedFollowing


def period(text):
    return ql.Period(text)


def handles(rows):
    return [[ql.QuoteHandle(ql.SimpleQuote(v)) for v in row] for row in rows]


ATM_OPTION_TENORS = [period(p) for p in ("1M", "6M", "1Y", "5Y", "10Y", "30Y")]
ATM_SWAP_TENORS = [period(p) for p in ("1Y", "5Y", "10Y", "30Y")]
ATM_VOLS = [
    [0.1300, 0.1560, 0.1390, 0.1220],
    [0.1440, 0.1580, 0.1460, 0.1260],
    [0.1600, 0.1590, 0.1470, 0.1290],
    [0.1640, 0.1470, 0.1370, 0.1220],
    [0.1400, 0.1300, 0.1250, 0.1100],
    [0.1130, 0.1090, 0.1070, 0.0930],
]
CUBE_OPTION_TENORS = [period(p) for p in ("1Y", "10Y", "30Y")]
CUBE_SWAP_TENORS = [period(p) for p in ("2Y", "10Y", "30Y")]
STRIKE_SPREADS = [-0.020, -0.005, 0.000, 0.005, 0.020]
VOL_SPREADS = [
    [0.0599, 0.0049, 0.0000, -0.0001, 0.0127],
    [0.0729, 0.0086, 0.0000, -0.0024, 0.0098],
    [0.0738, 0.0102, 0.0000, -0.0039, 0.0065],
    [0.0465, 0.0063, 0.0000, -0.0032, -0.0010],
    [0.0558, 0.0084, 0.0000, -0.0050, -0.0057],
    [0.0576, 0.0083, 0.0000, -0.0043, -0.0014],
    [0.0437, 0.0059, 0.0000, -0.0030, -0.0006],
    [0.0533, 0.0078, 0.0000, -0.0045, -0.0046],
    [0.0545, 0.0079, 0.0000, -0.0042, -0.0020],
]
QUERY_OPTION_TENORS = ["2Y", "5Y", "7Y", "20Y"]
QUERY_SWAP_TENORS = ["2Y", "5Y", "20Y"]
QUERY_STRIKE_OFFSETS = [-0.005, 0.0, 0.01]


def build_cube(is_atm_calibrated, backward_flat):
    curve = ql.YieldTermStructureHandle(
        ql.FlatForward(today, 0.05, ql.Actual360(), ql.Continuous, ql.Annual)
    )
    euribor6m = ql.Euribor6M(curve)
    atm = ql.SwaptionVolatilityStructureHandle(
        ql.SwaptionVolatilityMatrix(
            TARGET, BDC, ATM_OPTION_TENORS, ATM_SWAP_TENORS, handles(ATM_VOLS),
            ql.Actual365Fixed(), False, ql.ShiftedLognormal, [],
        )
    )

    def isda_index(tenor):
        return ql.SwapIndex(
            "EuriborSwapIsdaFixA", period(tenor), 2, ql.EURCurrency(), TARGET,
            period("1Y"), BDC, ql.Thirty360(ql.Thirty360.BondBasis), euribor6m,
        )

    return ql.SabrSwaptionVolatilityCube(
        atm, CUBE_OPTION_TENORS, CUBE_SWAP_TENORS, STRIKE_SPREADS,
        handles(VOL_SPREADS), isda_index("2Y"), isda_index("1Y"), False,
        handles([[0.2, 0.5, 0.4, 0.0]] * 9), [False] * 4, is_atm_calibrated,
        None, ql.nullDouble(), None, ql.nullDouble(), False, 50, backward_flat, 0.0001,
    )


rows = []
for is_atm_calibrated in (False, True):
    bilinear = build_cube(is_atm_calibrated, False)
    backward = build_cube(is_atm_calibrated, True)
    for option in QUERY_OPTION_TENORS:
        for swap in QUERY_SWAP_TENORS:
            option_date = bilinear.optionDateFromTenor(period(option))
            atm_strike = bilinear.atmStrike(option_date, period(swap))
            for offset in QUERY_STRIKE_OFFSETS:
                strike = atm_strike + offset
                vol_bilinear = bilinear.volatility(option_date, period(swap), strike, True)
                vol_backward = backward.volatility(option_date, period(swap), strike, True)
                on_dense_node = is_atm_calibrated and option == "5Y"
                gap = abs(vol_bilinear - vol_backward)
                assert gap < 1e-12 if on_dense_node else gap > 1e-3, (option, swap, offset, gap)
                rows.append([
                    int(is_atm_calibrated), option, swap, f"{strike:.17g}",
                    f"{vol_bilinear:.17g}", f"{vol_backward:.17g}",
                ])

with Path(__file__).with_name("oracle.csv").open("w", newline="") as stream:
    writer = csv.writer(stream, lineterminator="\n")
    writer.writerow([
        "is_atm_calibrated", "option_tenor", "swap_tenor", "strike",
        "vol_bilinear", "vol_backward_flat",
    ])
    writer.writerows(rows)
