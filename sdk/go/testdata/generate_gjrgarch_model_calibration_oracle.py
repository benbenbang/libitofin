"""Independent QuantLib original DAX and synthetic calibration reproductions."""

import QuantLib as ql

from generate_gjrgarch_model_oracle_support import (
    BASE,
    DATE,
    market,
    option,
    stationary,
)


def dax():
    date = ql.Date(5, 7, 2002)
    ql.Settings.instance().evaluationDate = date
    tenors = [13, 41, 75, 165, 256, 345, 524, 703]
    rates = [0.0357, 0.0349, 0.0341, 0.0355, 0.0359, 0.0368, 0.0386, 0.0401]
    dc, calendar = ql.Actual365Fixed(), ql.TARGET()
    rf = ql.YieldTermStructureHandle(
        ql.ZeroCurve([date] + [date + t for t in tenors], [rates[0]] + rates, dc)
    )
    dy = ql.YieldTermStructureHandle(ql.FlatForward(date, 0.0, dc))
    params = [0.000002, 0.024, 0.93, 0.059, 0.1, stationary(0.1)]
    proc = ql.GJRGARCHProcess(
        rf, dy, ql.QuoteHandle(ql.SimpleQuote(4468.17)), params[5], *params[:5], 365
    )
    model = ql.GJRGARCHModel(proc)
    engine = ql.AnalyticGJRGARCHEngine(model)
    strikes = [4000, 4200, 4400, 4500, 4600, 4800, 5000]
    vols = [
        [0.4541, 0.3869, 0.3492],
        [0.4060, 0.3607, 0.3330],
        [0.3726, 0.3396, 0.3108],
        [0.3550, 0.3277, 0.3012],
        [0.3428, 0.3209, 0.2958],
        [0.3302, 0.3062, 0.2799],
        [0.3343, 0.2959, 0.2705],
    ]
    helpers, rows = [], []
    for strike, values in zip(strikes, vols):
        for t, vol in zip(tenors[:3], values):
            weeks = int((t + 3) / 7)
            expiry = calendar.advance(date, ql.Period(weeks, ql.Weeks))
            helper = ql.HestonModelHelper(
                ql.Period(weeks, ql.Weeks),
                calendar,
                4468.17,
                strike,
                ql.QuoteHandle(ql.SimpleQuote(vol)),
                rf,
                dy,
                ql.BlackCalibrationHelper.ImpliedVolError,
            )
            helper.setPricingEngine(engine)
            helpers.append(helper)
            rows.append(
                dict(
                    strike=strike,
                    weeks=weeks,
                    maturity_days=expiry - date,
                    expiry=expiry.ISO(),
                    market_vol=vol,
                    market_price=helper.marketValue(),
                    risk_free_discount=rf.discount(expiry),
                    dividend_discount=dy.discount(expiry),
                    option_type=1 if strike * rf.discount(expiry) >= 4468.17 else -1,
                )
            )
    model.calibrate(
        helpers, ql.Simplex(0.05), ql.EndCriteria(400, 40, 1e-8, 1e-8, 1e-8)
    )
    for row, helper in zip(rows, helpers):
        row.update(
            model_price=helper.modelValue(), calibration_error=helper.calibrationError()
        )
    sse = sum((row["calibration_error"] * 100) ** 2 for row in rows)
    assert sse <= 15
    return dict(
        evaluation_date=date.ISO(),
        spot=4468.17,
        days_per_year=365,
        zero_curve_days=[0] + tenors,
        zero_curve_rates=[rates[0]] + rates,
        initial_params=params,
        calibrated_params=list(model.params()),
        solver="Simplex",
        simplex_lambda=0.05,
        max_iterations=400,
        stationary_iterations=40,
        tolerances=[1e-8] * 3,
        end_criteria=model.endCriteria(),
        function_evaluations=model.functionEvaluation(),
        percent_vol_sse=sse,
        rows=rows,
    )


def synthetic(free_omega=False):
    ql.Settings.instance().evaluationDate = DATE
    target = dict(BASE, dividend_yield=0.0)
    proc, _ = market(target)
    target_engine = ql.AnalyticGJRGARCHEngine(ql.GJRGARCHModel(proc))
    fit_proc, _ = market(
        dict(
            target,
            daily_variance=target["daily_variance"] * 0.7,
            omega=target["omega"] * (0.7 if free_omega else 1.0),
        )
    )
    model = ql.GJRGARCHModel(fit_proc)
    engine = ql.AnalyticGJRGARCHEngine(model)
    initial = list(model.params())
    helpers, rows = [], []
    for days in [14, 35, 63]:
        for strike in [90, 110]:
            kind = ql.Option.Put if strike < 100 else ql.Option.Call
            price = option(target_engine, strike, days, kind).NPV()
            helper = ql.HestonModelHelper(
                ql.Period(days, ql.Days),
                ql.NullCalendar(),
                100,
                strike,
                ql.QuoteHandle(ql.SimpleQuote(0.2)),
                proc.riskFreeRate(),
                proc.dividendYield(),
                ql.BlackCalibrationHelper.ImpliedVolError,
            )
            vol = helper.impliedVolatility(price, 1e-12, 1000, 0.001, 5)
            helper = ql.HestonModelHelper(
                ql.Period(days, ql.Days),
                ql.NullCalendar(),
                100,
                strike,
                ql.QuoteHandle(ql.SimpleQuote(vol)),
                proc.riskFreeRate(),
                proc.dividendYield(),
                ql.BlackCalibrationHelper.ImpliedVolError,
            )
            helper.setPricingEngine(engine)
            helpers.append(helper)
            rows.append(
                dict(
                    maturity_days=days,
                    strike=strike,
                    market_vol=vol,
                    target_price=price,
                )
            )
    fixed, weights = [not free_omega] + [True] * 4 + [False], [1, 2, 3, 4, 5, 6]
    model.calibrate(
        helpers,
        ql.Simplex(0.00002),
        ql.EndCriteria(500, 40, 1e-10, 1e-10, 1e-10),
        ql.NoConstraint(),
        weights,
        fixed,
    )
    for row, helper in zip(rows, helpers):
        row.update(
            model_price=helper.modelValue(), calibration_error=helper.calibrationError()
        )
    assert all(a == b for a, b, flag in zip(model.params(), initial, fixed) if flag)
    return dict(
        input=target,
        initial_params=initial,
        calibrated_params=list(model.params()),
        fixed_params=fixed,
        weights=weights,
        solver="Simplex",
        simplex_lambda=0.00002,
        max_iterations=500,
        stationary_iterations=40,
        tolerances=[1e-10] * 3,
        weighted_sse=sum(
            w * h.calibrationError() ** 2 for w, h in zip(weights, helpers)
        ),
        rows=rows,
    )
