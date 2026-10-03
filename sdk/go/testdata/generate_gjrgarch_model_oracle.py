"""Generate independent model and engine fixtures using QuantLib 1.43."""

import argparse
import hashlib
import json
import math
from pathlib import Path

import QuantLib as ql

from generate_gjrgarch_model_calibration_oracle import dax, synthetic
from generate_gjrgarch_model_oracle_support import (
    ANALYTIC,
    BASE,
    COMMIT,
    DATE,
    MC,
    market,
    option,
    replay_count,
    stationary,
)


def matrix():
    for i, lam in enumerate([0.0, 0.1, 0.2]):
        config = dict(
            BASE,
            spot=50.0,
            daily_variance=stationary(lam),
            risk_free_rate=0.05,
            dividend_yield=0.0,
            alpha=0.024,
            beta=0.93,
            gamma=0.059,
            lambda_parameter=lam,
            days_per_year=365.0,
        )
        proc, _ = market(config, dc=ql.ActualActual(ql.ActualActual.ISDA))
        model = ql.GJRGARCHModel(proc)
        engine = ql.AnalyticGJRGARCHEngine(model)
        mc_engine = ql.MCPREuropeanGJRGARCHEngine(
            proc, timeStepsPerYear=20, requiredTolerance=0.02, seed=1234
        )
        rows = []
        for days in [90, 180]:
            for strike in [35, 40, 45, 50, 55, 60]:
                j = len(rows)
                native_mc = option(mc_engine, strike, days)
                call, mc_value = option(engine, strike, days).NPV(), native_mc.NPV()
                assert abs(call - ANALYTIC[i][j]) <= 0.15
                assert abs(mc_value - MC[i][j]) <= 0.15
                rows.append(
                    dict(
                        maturity_days=days,
                        strike=strike,
                        call=call,
                        put=option(engine, strike, days, ql.Option.Put).NPV(),
                        cached_analytic=ANALYTIC[i][j],
                        cached_mc=MC[i][j],
                        mc_price=mc_value,
                        mc_error=native_mc.errorEstimate(),
                        mc_samples=replay_count(
                            proc,
                            strike,
                            days,
                            mc_value,
                            native_mc.errorEstimate(),
                            0.02,
                            dict(timeStepsPerYear=20, seed=1234),
                        ),
                    )
                )
        yield (
            f"matrix-{i}",
            dict(input=config, day_counter="ActualActual ISDA", rows=rows),
        )


def analytic_cases():
    rows = []
    for name, changes, days in [
        ("base", {}, 30),
        ("ninety", {}, 90),
        ("one_eighty", {}, 180),
        ("one_year", {}, 365),
        ("positive_lambda", dict(lambda_parameter=0.3), 90),
        ("zero_lambda", dict(lambda_parameter=0.0), 90),
        ("negative_gamma", dict(gamma=-0.02, lambda_parameter=0.3), 90),
        ("noninteger_days", dict(days_per_year=252.5), 91),
        ("negative_rate", dict(risk_free_rate=-0.01), 30),
        ("high_dividend", dict(dividend_yield=0.08), 90),
        ("nonstationary", dict(beta=0.99), 30),
        ("one_day", {}, 1),
    ]:
        config = dict(BASE, **changes)
        proc, _ = market(config)
        engine = ql.AnalyticGJRGARCHEngine(ql.GJRGARCHModel(proc))
        strike = 0 if name == "zero_strike" else 100
        rows.append(
            dict(
                name=name,
                changes=changes,
                maturity_days=days,
                strike=strike,
                call=option(engine, strike, days).NPV(),
                put=option(engine, strike, days, ql.Option.Put).NPV(),
                parity=100
                - strike
                * math.exp(
                    -(config["risk_free_rate"] - config["dividend_yield"]) * days / 365
                ),
            )
        )
    proc, quotes = market(BASE)
    model = ql.GJRGARCHModel(proc)
    live = option(ql.AnalyticGJRGARCHEngine(model), 100, 90)
    bumps = [dict(name="initial", price=live.NPV())]
    for name, quote, value in zip(
        ["spot", "risk_free_rate", "dividend_yield"], quotes, [105, 0.05, 0.03]
    ):
        quote.setValue(value)
        bumps.append(
            dict(
                name=name,
                value=value,
                native_cached_price=live.NPV(),
                price=option(ql.AnalyticGJRGARCHEngine(model), 100, 90).NPV(),
            )
        )
    params = list(model.params())
    params[0] *= 1.1
    model.setParams(ql.Array(params))
    bumps.append(dict(name="omega", value=params[0], price=live.NPV()))
    return dict(input=BASE, rows=rows, cumulative_live_bumps=bumps)


def mc_cases():
    rows = []
    for scheme in range(3):
        for antithetic in [False, True]:
            config = dict(
                BASE,
                alpha=0.3,
                beta=0.4,
                gamma=0.9,
                daily_variance=0.002,
                discretization=scheme,
            )
            proc, _ = market(config)
            settings = dict(
                timeSteps=4, antitheticVariate=antithetic, requiredSamples=2048, seed=42
            )
            for kind in [ql.Option.Call, ql.Option.Put]:
                engine = ql.MCPREuropeanGJRGARCHEngine(proc, **settings)
                opt = option(engine, 100, 7, kind)
                rows.append(
                    dict(
                        input=config,
                        settings=settings,
                        option_type=kind,
                        strike=100,
                        maturity_days=7,
                        price=opt.NPV(),
                        error=opt.errorEstimate(),
                        effective_samples=2048,
                    )
                )
    proc, _ = market(BASE)
    tolerance_settings = dict(
        timeSteps=8, requiredTolerance=0.04, maxSamples=100000, seed=1234
    )
    opt = option(ql.MCPREuropeanGJRGARCHEngine(proc, **tolerance_settings), 100, 90)
    price, error = opt.NPV(), opt.errorEstimate()
    count = replay_count(
        proc, 100, 90, price, error, 0.04, dict(timeSteps=8, seed=1234)
    )
    return dict(
        rows=rows,
        tolerance=dict(
            input=BASE,
            settings=tolerance_settings,
            strike=100,
            maturity_days=90,
            price=price,
            error=error,
            effective_samples=count,
        ),
    )


def edge_cases():
    rows = []
    for name, changes, strike in [
        ("zero_strike", {}, 0),
        ("equal_moments", dict(alpha=0.0, beta=1.0, gamma=0.0), 100),
        ("zero_moments", dict(alpha=0.0, beta=0.0, gamma=0.0), 100),
        ("zero_v0", dict(daily_variance=0.0), 100),
    ]:
        proc, _ = market(dict(BASE, **changes))
        try:
            engine = ql.AnalyticGJRGARCHEngine(ql.GJRGARCHModel(proc))
            value = option(engine, strike, 90).NPV()
        except RuntimeError as error:
            value = str(error)
        rows.append(
            dict(
                name=name,
                changes=changes,
                strike=strike,
                maturity_days=90,
                native_result="nan"
                if isinstance(value, float) and math.isnan(value)
                else value,
            )
        )
    proc, _ = market(dict(BASE, discretization=2))
    model = ql.GJRGARCHModel(proc)
    state, dw = ql.Array([100, -0.04]), ql.Array([0, 0])
    reset = dict(
        input_scheme=2,
        expected_model_scheme=1,
        input_zero_dt=list(proc.evolve(0, state, 0, dw)),
        model_zero_dt=list(model.process().evolve(0, state, 0, dw)),
    )
    invalid = []
    for index, value in [(0, 0.0), (1, 1.1), (2, 1.1), (3, -1.0), (5, 0.0)]:
        params = list(ql.GJRGARCHModel(proc).params())
        params[index] = value
        model.setParams(ql.Array(params))
        invalid.append(
            dict(
                params=params,
                native_setter_accepted=list(model.params()) == params,
                calibration_constraint_valid=False,
            )
        )
    return dict(input=BASE, rows=rows, model_reset=reset, invalid_setter_cases=invalid)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    if ql.__version__ != "1.43":
        raise RuntimeError(f"QuantLib 1.43 required, got {ql.__version__}")
    ql.Settings.instance().evaluationDate = DATE
    documents = dict(matrix())
    documents.update(
        analytic=analytic_cases(),
        mc=mc_cases(),
        dax=dax(),
        synthetic=synthetic(),
        **{"synthetic-free-omega": synthetic(free_omega=True)},
        edge=edge_cases(),
    )
    args.output.mkdir(parents=True, exist_ok=True)
    for name, data in documents.items():
        data.update(
            quantlib_version=ql.__version__,
            contract_commit=COMMIT,
            reference_date=data.get("evaluation_date", DATE.ISO()),
            day_counter=data.get("day_counter", "Actual365Fixed"),
        )
        path = args.output / f"gjrgarch-model-{name}.json"
        path.write_text(
            json.dumps(data, sort_keys=True, indent=2, allow_nan=False) + "\n"
        )
        print(path.name, hashlib.sha256(path.read_bytes()).hexdigest())


if __name__ == "__main__":
    main()
