"""Generate independent retained-batch numerical references with QuantLib 1.43."""

import json
from pathlib import Path

import QuantLib as ql

assert ql.__version__ == "1.43"
DESTINATION = Path(__file__).parent


def outcome(function):
    try:
        return {"value": function()}
    except RuntimeError as error:
        return {"error": str(error)}


def emit(name, inputs, rows):
    document = {
        "quantlib_version": ql.__version__,
        "local_source_revision": "9863b578af0caa4cecabf697196533e84a8308b6",
        "local_source_version": "1.43-dev",
        "compiled_oracle": "PyPI QuantLib 1.43 Python bindings and compiled library",
        "inputs": inputs,
        "rows": rows,
    }
    (DESTINATION / name).write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n"
    )


def fx_delta():
    inputs = dict(
        spot=1.421,
        domestic_discount=0.997306,
        foreign_discount=0.992266,
        stddev=0.1180654,
        strike=1.6,
    )
    rows = []
    for option, option_type in [("call", ql.Option.Call), ("put", ql.Option.Put)]:
        for convention in ["Spot", "Fwd", "PaSpot", "PaFwd"]:
            calculator = ql.BlackDeltaCalculator(
                option_type,
                getattr(ql.DeltaVolQuote, convention),
                inputs["spot"],
                inputs["domestic_discount"],
                inputs["foreign_discount"],
                inputs["stddev"],
            )
            delta = 0.15 * option_type
            row = dict(
                option=option,
                convention=convention,
                target_delta=delta,
                delta_at_strike=calculator.deltaFromStrike(inputs["strike"]),
                strike_from_delta=outcome(lambda: calculator.strikeFromDelta(delta)),
                atm={},
            )
            for atm in [
                "AtmSpot",
                "AtmFwd",
                "AtmDeltaNeutral",
                "AtmGammaMax",
                "AtmVegaMax",
                "AtmPutCall50",
                "AtmNull",
            ]:
                row["atm"][atm] = outcome(
                    lambda atm=atm: calculator.atmStrike(getattr(ql.DeltaVolQuote, atm))
                )
            if option == "call" and convention.startswith("Pa"):
                row["infeasible_delta_0_9"] = outcome(
                    lambda: calculator.strikeFromDelta(0.9)
                )
            rows.append(row)
    emit("fx-delta.json", inputs, rows)


def cashflow_spreads():
    reference = ql.Date(1, 7, 2026)
    settlement, npv_date = ql.Date(7, 7, 2026), ql.Date(9, 7, 2026)
    dc = ql.Actual360()
    curve = ql.FlatForward(reference, 0.03, dc, ql.Continuous)
    leg = [
        ql.SimpleCashFlow(amount, date)
        for amount, date in [
            (7.0, settlement),
            (20.0, ql.Date(7, 12, 2026)),
            (100.0, ql.Date(7, 7, 2027)),
        ]
    ]
    inputs = dict(
        reference="2026-07-01",
        settlement="2026-07-07",
        npv_date="2026-07-09",
        base_rate=0.03,
        base_compounding="Continuous",
        day_counter="Actual360",
        frequency="Semiannual",
        leg=[[7, "2026-07-07"], [20, "2026-12-07"], [100, "2027-07-07"]],
    )
    rows = []
    for compounding in [
        "Simple",
        "Compounded",
        "Continuous",
        "SimpleThenCompounded",
        "CompoundedThenSimple",
    ]:
        for spread in [-0.015, 0.025]:
            for include in [False, True]:
                convention = getattr(ql, compounding)
                value = ql.CashFlows.npv(
                    leg,
                    curve,
                    spread,
                    dc,
                    convention,
                    ql.Semiannual,
                    include,
                    settlement,
                    npv_date,
                )
                recovered = ql.CashFlows.zSpread(
                    leg,
                    value,
                    curve,
                    dc,
                    convention,
                    ql.Semiannual,
                    include,
                    settlement,
                    npv_date,
                    1e-12,
                    100,
                    0.0,
                )
                assert abs(recovered - spread) < 1e-11
                rows.append(
                    dict(
                        compounding=compounding,
                        spread=spread,
                        include_settlement_flows=include,
                        npv=value,
                        recovered_spread=recovered,
                    )
                )
    emit("cashflow-spreads.json", inputs, rows)


def bond_quotes():
    ql.Settings.instance().evaluationDate = ql.Date(1, 7, 2026)
    first, middle, last = ql.Date(7, 7, 2026), ql.Date(7, 7, 2027), ql.Date(7, 7, 2028)
    dc = ql.Actual360()
    coupons = [
        ql.FixedRateCoupon(
            middle, 1000, 0.05, dc, first, middle, first, middle, ql.Date(1, 7, 2027)
        ),
        ql.FixedRateCoupon(
            last, 1000, 0.05, dc, middle, last, middle, last, ql.Date(1, 7, 2028)
        ),
    ]
    ex_bond = ql.Bond(0, ql.NullCalendar(), first, coupons)
    schedule = ql.Schedule(
        first,
        last,
        ql.Period(1, ql.Years),
        ql.NullCalendar(),
        ql.Unadjusted,
        ql.Unadjusted,
        ql.DateGeneration.Forward,
        False,
    )
    amortizing = ql.AmortizingFixedRateBond(
        0, [1000, 600], schedule, [0.05], dc, ql.Unadjusted, first
    )
    curve = ql.FlatForward(ql.Date(1, 7, 2026), 0.03, dc)
    interest = ql.InterestRate(0.045, dc, ql.Compounded, ql.Semiannual)
    rows = []
    for name, bond, settlements in [
        ("ex_coupon", ex_bond, [ql.Date(3, 7, 2027)]),
        ("amortizing", amortizing, [middle, ql.Date(10, 7, 2027)]),
    ]:
        for settlement in settlements:
            notional = bond.notional(settlement)
            dirty_yield = (
                ql.CashFlows.npv(
                    bond.cashflows(), interest, False, settlement, settlement
                )
                * 100
                / notional
            )
            accrued = bond.accruedAmount(settlement)
            clean_yield = ql.BondFunctions.cleanPrice(bond, interest, settlement)
            assert abs(dirty_yield - accrued - clean_yield) < 1e-11
            rows.append(
                dict(
                    name=name,
                    settlement=settlement.ISO(),
                    notional=notional,
                    accrued=accrued,
                    dirty_yield=dirty_yield,
                    clean_yield=clean_yield,
                    recovered_clean_yield=ql.BondFunctions.bondYield(
                        bond,
                        ql.BondPrice(clean_yield, ql.BondPrice.Clean),
                        dc,
                        ql.Compounded,
                        ql.Semiannual,
                        settlement,
                        1e-12,
                    ),
                    curve_clean=ql.BondFunctions.cleanPrice(bond, curve, settlement),
                    curve_dirty=ql.BondFunctions.dirtyPrice(bond, curve, settlement),
                    cashflows=[
                        dict(date=cf.date().ISO(), amount=cf.amount())
                        for cf in bond.cashflows()
                    ],
                )
            )
    emit(
        "bond-quotes.json",
        dict(
            day_counter="Actual360",
            yield_rate=0.045,
            yield_compounding="Compounded",
            yield_frequency="Semiannual",
            base_rate=0.03,
            base_compounding="Continuous",
            coupon_rate=0.05,
            schedule=[first.ISO(), middle.ISO(), last.ISO()],
            ex_coupon_dates=["2027-07-01", "2028-07-01"],
        ),
        rows,
    )


def callable_prices():
    reference = ql.Date(7, 7, 2026)
    ql.Settings.instance().evaluationDate = reference
    dc = ql.Actual365Fixed()
    curve = ql.YieldTermStructureHandle(ql.FlatForward(reference, 0.03, dc))
    model = ql.HullWhite(curve, 0.1, 0.01)
    maturity = ql.Date(7, 7, 2031)
    schedule = ql.Schedule(
        reference,
        maturity,
        ql.Period(6, ql.Months),
        ql.NullCalendar(),
        ql.Unadjusted,
        ql.Unadjusted,
        ql.DateGeneration.Forward,
        False,
    )
    rows = []
    for name, kind, price_type, call_price, date in [
        ("noncallable", None, ql.BondPrice.Clean, 100, ql.Date(7, 7, 2028)),
        (
            "call_clean",
            ql.Callability.Call,
            ql.BondPrice.Clean,
            100,
            ql.Date(7, 7, 2028),
        ),
        ("put_clean", ql.Callability.Put, ql.BondPrice.Clean, 100, ql.Date(7, 7, 2028)),
        (
            "call_clean_off_coupon",
            ql.Callability.Call,
            ql.BondPrice.Clean,
            100,
            ql.Date(7, 8, 2028),
        ),
        (
            "call_dirty_off_coupon",
            ql.Callability.Call,
            ql.BondPrice.Dirty,
            100,
            ql.Date(7, 8, 2028),
        ),
        (
            "call_close_coupon",
            ql.Callability.Call,
            ql.BondPrice.Clean,
            100,
            ql.Date(5, 7, 2028),
        ),
    ]:
        callability = ql.CallabilitySchedule()
        if kind is not None:
            callability.append(
                ql.Callability(ql.BondPrice(call_price, price_type), kind, date)
            )
        bond = ql.CallableFixedRateBond(
            2, 1000, schedule, [0.05], dc, ql.Unadjusted, 100, reference, callability
        )
        bond.setPricingEngine(ql.TreeCallableFixedRateBondEngine(model, 100))
        rows.append(
            dict(
                name=name,
                call_date=date.ISO(),
                call_price=call_price,
                npv=bond.NPV(),
                settlement_value=bond.settlementValue(),
                dirty=bond.dirtyPrice(),
                clean=bond.cleanPrice(),
                accrued=bond.accruedAmount(),
                settlement=bond.settlementDate().ISO(),
            )
        )
    emit(
        "callable-prices.json",
        dict(
            reference=reference.ISO(),
            maturity=maturity.ISO(),
            day_counter="Actual365Fixed",
            coupon=0.05,
            tenor_months=6,
            face=1000,
            redemption_per_100=100,
            risk_free=0.03,
            hull_white_a=0.1,
            hull_white_sigma=0.01,
            tree_steps=100,
            settlement_days=2,
            calendar="NullCalendar",
            convention="Unadjusted",
        ),
        rows,
    )


if __name__ == "__main__":
    fx_delta()
    cashflow_spreads()
    bond_quotes()
    callable_prices()
