"""Show why generic bond coupon analytics are not raw CPI-rate sensitivities."""

import hashlib
import json
import sys
from pathlib import Path

import QuantLib as q
import QuantLib._QuantLib as native
from generate_cpi_oracle import DC, EVALUATION, LAG, SOURCE_REVISION, schedule, setup

rows = []
for interpolation, label in [(q.CPI.Flat, "flat"), (q.CPI.Linear, "linear")]:
    index, zh, nominal, quote = setup()

    def bond(rate: float, index=index, interpolation=interpolation, nominal=nominal):
        """Rebuild the native instrument at a contractual fixed coupon rate."""
        result = q.CPIBond(
            2,
            100000.0,
            False,
            100.0,
            LAG,
            index,
            interpolation,
            schedule(),
            [rate],
            DC,
            q.Unadjusted,
            q.Date(15, 1, 2025),
            q.NullCalendar(),
        )
        result.setPricingEngine(q.DiscountingBondEngine(nominal))
        return result

    original, bumped, target = bond(0.02), bond(0.0201), bond(0.03)
    settlement = original.settlementDate()
    curve = nominal.currentLink()
    normalized_bps = q.BondFunctions.bps(original, curve, settlement)
    currency_bps = q.CashFlows.bps(
        original.cashflows(), nominal, False, EVALUATION, EVALUATION
    )
    raw_currency_bump = bumped.NPV() - original.NPV()
    raw_dirty_price_bump = bumped.dirtyPrice() - original.dirtyPrice()
    yield_bps = q.BondFunctions.bps(
        original, 0.04, DC, q.Continuous, q.Annual, settlement
    )
    original_yield_npv = q.CashFlows.npv(
        original.cashflows(),
        0.04,
        DC,
        q.Continuous,
        q.Annual,
        False,
        settlement,
        settlement,
    )
    bumped_yield_npv = q.CashFlows.npv(
        bumped.cashflows(),
        0.04,
        DC,
        q.Continuous,
        q.Annual,
        False,
        settlement,
        settlement,
    )
    yield_dirty_price_bump = (
        (bumped_yield_npv - original_yield_npv) * 100 / original.notional(settlement)
    )
    own_atm = q.BondFunctions.atmRate(original, curve, settlement)
    target_dirty_atm = q.BondFunctions.atmRate(
        original, curve, settlement, q.BondPrice(target.dirtyPrice(), q.BondPrice.Dirty)
    )
    target_clean_atm = q.BondFunctions.atmRate(
        original, curve, settlement, q.BondPrice(target.cleanPrice(), q.BondPrice.Clean)
    )
    rows.append(
        {
            "interpolation": label,
            "original_raw_fixed_rate": 0.02,
            "bumped_raw_fixed_rate": 0.0201,
            "target_raw_fixed_rate": 0.03,
            "settlement": settlement.ISO(),
            "original_npv": original.NPV(),
            "bumped_npv": bumped.NPV(),
            "generic_currency_coupon_bps": currency_bps,
            "actual_currency_raw_coupon_1bp_bump": raw_currency_bump,
            "generic_per100_coupon_bps": normalized_bps,
            "actual_dirty_per100_raw_coupon_1bp_bump": raw_dirty_price_bump,
            "generic_yield_coupon_bps": yield_bps,
            "actual_yield_dirty_per100_raw_coupon_1bp_bump": yield_dirty_price_bump,
            "generic_own_npv_atm_rate": own_atm,
            "generic_target_dirty_price_atm_rate": target_dirty_atm,
            "generic_target_clean_price_atm_rate": target_clean_atm,
            "effective_coupon_bps_scaling": raw_currency_bump / currency_bps,
            "own_atm_to_raw_rate_ratio": own_atm / 0.02,
        }
    )
output = {
    "schema": 1,
    "provenance": {
        "wheel_version": q.__version__,
        "native_sha256": hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest(),
        "source_reference_revision": SOURCE_REVISION,
        "source_is_wheel_build_provenance": False,
    },
    "source_generator": "scripts/generate_cpi_bond_coupon_risk_oracle.py",
    "inputs": "cpi-oracle-inputs.json",
    "method": "native QuantLib BondFunctions and CashFlows results compared with rebuilding CPIBond at raw fixed rate plus 1bp; no Rust implementation involved",
    "conclusion": "generic coupon BPS omits CPI index ratios and generic ATM returns nominal/accrual-weighted effective coupon rate, not raw CPI fixed rate; these are not raw CPI fixed-rate sensitivity/par-rate analytics",
    "rows": rows,
}
path = Path(sys.argv[1])
path.write_text(json.dumps(output, indent=2, sort_keys=True, allow_nan=False) + "\n")
print(path, hashlib.sha256(path.read_bytes()).hexdigest())
for row in rows:
    print(
        row["interpolation"],
        "generic/actual currency BPS",
        row["generic_currency_coupon_bps"],
        row["actual_currency_raw_coupon_1bp_bump"],
        "ATM/raw",
        row["generic_own_npv_atm_rate"],
        row["original_raw_fixed_rate"],
        "price-fed ATM/raw",
        row["generic_target_dirty_price_atm_rate"],
        row["target_raw_fixed_rate"],
    )
