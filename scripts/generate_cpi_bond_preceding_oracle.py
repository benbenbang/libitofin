"""Reproduce bounded CPI-bond preceding-payment profiles using QuantLib 1.43."""

import hashlib
import json
import sys
from pathlib import Path

import QuantLib as q
from generate_cpi_oracle import DC, LAG, bond_snapshot, setup

rows = []
for tenor_months, rates in [(12, [0.02]), (6, [0.02, 0.03])]:
    index, zh, nominal, quote = setup()
    schedule = q.Schedule(
        q.Date(15, 1, 2025),
        q.Date(19, 7, 2025),
        q.Period(tenor_months, q.Months),
        q.NullCalendar(),
        q.Unadjusted,
        q.Unadjusted,
        q.DateGeneration.Forward,
        False,
    )
    bond = q.CPIBond(
        0,
        1000.0,
        False,
        100.0,
        LAG,
        index,
        q.CPI.Linear,
        schedule,
        rates,
        DC,
        q.Preceding,
        q.Date(15, 1, 2025),
        q.WeekendsOnly(),
    )
    bond.setPricingEngine(q.DiscountingBondEngine(nominal))
    initial = bond_snapshot(bond)
    observations = []
    for include_reference in [False, True]:
        q.Settings.instance().includeReferenceDateEvents = include_reference
        q.Settings.instance().evaluationDate = q.Date(18, 7, 2025)
        observations.append(
            {
                "include_reference_date_events": include_reference,
                "notional_jul17": bond.notional(q.Date(17, 7, 2025)),
                "notional_jul18": bond.notional(q.Date(18, 7, 2025)),
                "notional_jul19": bond.notional(q.Date(19, 7, 2025)),
                "maturity": bond.maturityDate().ISO(),
                "expired": bond.isExpired(),
                "cashflow_npv_include_settlement": q.CashFlows.npv(
                    bond.cashflows(),
                    nominal,
                    True,
                    q.Date(18, 7, 2025),
                    q.Date(18, 7, 2025),
                ),
                "cashflow_npv_exclude_settlement": q.CashFlows.npv(
                    bond.cashflows(),
                    nominal,
                    False,
                    q.Date(18, 7, 2025),
                    q.Date(18, 7, 2025),
                ),
            }
        )
    rows.append(
        {
            "tenor_months": tenor_months,
            "rates": rates,
            "schedule": [d.ISO() for d in schedule],
            "initial": initial,
            "terminal": observations,
        }
    )
output = {
    "schema": 1,
    "source_generator": "scripts/generate_cpi_bond_preceding_oracle.py",
    "shared_inputs": "cpi-oracle-inputs.json",
    "modifications": {
        "face_amount": 1000,
        "settlement_days": 0,
        "maturity": "2025-07-19",
        "payment_calendar": "WeekendsOnly",
        "payment_convention": "Preceding",
        "interpolation": "Linear",
    },
    "rows": rows,
}
path = Path(sys.argv[1])
path.write_text(json.dumps(output, indent=2, allow_nan=False, sort_keys=True) + "\n")
print(path, hashlib.sha256(path.read_bytes()).hexdigest())
