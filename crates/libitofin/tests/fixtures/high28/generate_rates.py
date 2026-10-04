"""Replay retained swaption, coupon-convention and swap-stub references."""

import QuantLib as ql

from generate import emit


def swaption_quotes():
    reference = ql.Date(7, 7, 2026)
    exercise_date = ql.Date(7, 7, 2027)
    ql.Settings.instance().evaluationDate = reference
    ql.IborCoupon.createAtParCoupons()
    curve = ql.YieldTermStructureHandle(ql.FlatForward(reference, 0.02, ql.Actual360()))
    index = ql.Euribor6M(curve)
    swap = ql.MakeVanillaSwap(
        ql.Period(5, ql.Years), index, 0.03, effectiveDate=ql.Date(9, 7, 2027)
    )
    rows = []
    for normal, vol, shift in [(False, 0.2, 0), (False, 0.3, 0.01), (True, 0.01, 0)]:
        for cash in [False, True]:
            swaption = ql.Swaption(
                swap,
                ql.EuropeanExercise(exercise_date),
                ql.Settlement.Cash if cash else ql.Settlement.Physical,
                ql.Settlement.ParYieldCurve if cash else ql.Settlement.PhysicalOTC,
            )
            quote = ql.QuoteHandle(ql.SimpleQuote(vol))
            engine = (
                ql.BachelierSwaptionEngine(curve, quote)
                if normal
                else ql.BlackSwaptionEngine(curve, quote, ql.Actual365Fixed(), shift)
            )
            swaption.setPricingEngine(engine)
            rows.append(
                dict(
                    normal=normal,
                    vol=vol,
                    shift=shift,
                    cash=cash,
                    npv=swaption.NPV(),
                    forward=swaption.NPV() / curve.discount(exercise_date),
                )
            )
    emit(
        "rates-swaptions.json",
        dict(
            reference=reference.ISO(),
            risk_free=0.02,
            curve_day_counter="Actual360",
            volatility_day_counter="Actual365Fixed",
            index="Euribor6M",
            swap_tenor_years=5,
            fixed_rate=0.03,
            effective="2027-07-09",
            exercise=exercise_date.ISO(),
            cash_settlement_method="ParYieldCurve",
            physical_settlement_method="PhysicalOTC",
            using_at_par_coupons=True,
        ),
        rows,
    )


def coupon_conventions():
    rows = []
    for name, reference, rate, start, end in [
        (
            "future_fixing",
            ql.Date(30, 6, 2026),
            0.02,
            ql.Date(3, 7, 2026),
            ql.Date(4, 1, 2027),
        ),
        (
            "holiday_value_date",
            ql.Date(6, 11, 2019),
            0.03,
            ql.Date(11, 11, 2019),
            ql.Date(11, 5, 2020),
        ),
    ]:
        ql.Settings.instance().evaluationDate = reference
        curve = ql.YieldTermStructureHandle(
            ql.FlatForward(reference, rate, ql.Actual360())
        )
        index = ql.USDLibor(ql.Period(6, ql.Months), curve)
        coupon = ql.IborCoupon(end, 100, start, end, 2, index)
        coupon.setPricer(ql.BlackIborCouponPricer())
        rows.append(
            dict(
                name=name,
                reference=reference.ISO(),
                curve_rate=rate,
                start=start.ISO(),
                end=end.ISO(),
                fixing=coupon.fixingDate().ISO(),
                index_value_date=index.valueDate(coupon.fixingDate()).ISO(),
                rate=coupon.rate(),
            )
        )
    fixing_rows = []
    for arrears in [False, True]:
        for convention in ["Preceding", "Following", "ModifiedFollowing"]:
            index = ql.Euribor6M(curve)
            coupon = ql.IborCoupon(
                ql.Date(2, 8, 2026),
                100,
                ql.Date(1, 2, 2026),
                ql.Date(1, 8, 2026),
                0,
                index,
                1,
                0,
                ql.Date(),
                ql.Date(),
                ql.Actual360(),
                arrears,
                ql.Date(),
                getattr(ql, convention),
            )
            fixing_rows.append(
                dict(
                    arrears=arrears,
                    convention=convention,
                    fixing=coupon.fixingDate().ISO(),
                )
            )
    emit(
        "rates-coupons.json",
        dict(
            day_counter="Actual360",
            notional=100,
            rate_cases_index="USDLibor6M",
            rate_cases_fixing_days=2,
            fixing_cases_index="Euribor6M",
            fixing_cases_fixing_days=0,
            fixing_cases_accrual_start="2026-02-01",
            fixing_cases_accrual_end="2026-08-01",
            fixing_cases_payment="2026-08-02",
        ),
        dict(coupon_rates=rows, fixing_dates=fixing_rows),
    )


def stub_schedules():
    rows = []
    for rule in ["Forward", "Backward"]:
        for years, first, next_to_last in [
            (True, ql.Date(15, 7, 2026), ql.Date(15, 10, 2028)),
            (False, ql.Date(15, 4, 2026), ql.Date(15, 11, 2028)),
        ]:
            schedule = ql.Schedule(
                ql.Date(15, 1, 2026),
                ql.Date(15, 1, 2029),
                ql.Period(1, ql.Years) if years else ql.Period(6, ql.Months),
                ql.NullCalendar(),
                ql.Unadjusted,
                ql.Unadjusted,
                getattr(ql.DateGeneration, rule),
                False,
                first,
                next_to_last,
            )
            rows.append(
                dict(
                    rule=rule,
                    tenor_months=12 if years else 6,
                    first_date=first.ISO(),
                    next_to_last_date=next_to_last.ISO(),
                    dates=[date.ISO() for date in schedule],
                )
            )
    emit(
        "rates-stub-schedules.json",
        dict(
            effective="2026-01-15",
            termination="2029-01-15",
            calendar="NullCalendar",
            convention="Unadjusted",
            end_of_month=False,
        ),
        rows,
    )


if __name__ == "__main__":
    swaption_quotes()
    coupon_conventions()
    stub_schedules()
