"""Reproduce independent QuantLib observations, not a direct helper call."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import QuantLib as ql

PIN = "9863b578af0caa4cecabf697196533e84a8308b6"


def formula(process, value, delta, gamma):
    spot = process.stateVariable().value()
    r = process.riskFreeRate().zeroRate(0.0, ql.Continuous).rate()
    q = process.dividendYield().zeroRate(0.0, ql.Continuous).rate()
    vol = process.localVolatility().localVol(0.0, spot)
    return r * value - (r - q) * spot * delta - 0.5 * vol * vol * spot * spot * gamma


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(
        ["git", "-C", str(args.source_root), "rev-parse", "HEAD"], text=True
    ).strip()
    assert revision == PIN, revision
    assert ql.__version__ == "1.43", ql.__version__
    source = args.source_root / "ql/pricingengines/greeks.cpp"
    source_bytes = source.read_bytes()
    assert b"return r*value -(r-q)*u*delta - 0.5*v*v*u*u*gamma;" in source_bytes
    reference = ql.Date(9, 10, 2026)
    ql.Settings.instance().evaluationDate = reference
    dc = ql.Actual365Fixed()
    spot = ql.SimpleQuote(100.0)
    r_quote = ql.SimpleQuote(0.05)
    q_quote = ql.SimpleQuote(0.02)
    v_quote = ql.SimpleQuote(0.20)
    risk = ql.RelinkableYieldTermStructureHandle(
        ql.FlatForward(reference, ql.QuoteHandle(r_quote), dc)
    )
    dividend = ql.RelinkableYieldTermStructureHandle(
        ql.FlatForward(reference, ql.QuoteHandle(q_quote), dc)
    )
    black = ql.BlackVolTermStructureHandle(
        ql.BlackConstantVol(reference, ql.NullCalendar(), ql.QuoteHandle(v_quote), dc)
    )
    process = ql.GeneralizedBlackScholesProcess(
        ql.QuoteHandle(spot), dividend, risk, black
    )
    rows = []
    for label, kind, strike, days in [
        ("call_atm", ql.Option.Call, 100.0, 365),
        ("put_itm", ql.Option.Put, 110.0, 182),
        ("call_otm", ql.Option.Call, 125.0, 730),
        ("put_short", ql.Option.Put, 95.0, 30),
    ]:
        option = ql.VanillaOption(
            ql.PlainVanillaPayoff(kind, strike), ql.EuropeanExercise(reference + days)
        )
        option.setPricingEngine(ql.AnalyticEuropeanEngine(process))
        value, delta, gamma = option.NPV(), option.delta(), option.gamma()
        identity = formula(process, value, delta, gamma)
        assert abs(identity - option.theta()) < 3e-10
        rows.append(dict(name=label, value=value, delta=delta, gamma=gamma,
                         theta=option.theta(), source_identity=identity))
    observations = []
    for label, updates in [("initial", (100, .05, .02, .2)),
                           ("updated", (115, -.01, .03, .35))]:
        for quote, value in zip((spot, r_quote, q_quote, v_quote), updates):
            quote.setValue(value)
        observations.append(dict(name=label, theta=formula(process, 12, .6, .02)))
    dates = [reference, reference + 182, reference + 365]
    risk.linkTo(ql.ZeroCurve(dates, [.03, .07, .09], dc))
    dividend.linkTo(ql.ZeroCurve(dates, [.01, .025, .04], dc))
    observations.append(dict(name="nonflat", theta=formula(process, 12, .6, .02),
                             r0=risk.zeroRate(0., ql.Continuous).rate(),
                             q0=dividend.zeroRate(0., ql.Continuous).rate()))
    local = ql.LocalVolTermStructureHandle(ql.LocalConstantVol(reference, .12, dc))
    external = ql.GeneralizedBlackScholesProcess(
        ql.QuoteHandle(spot), dividend, risk, black, local
    )
    observations.append(dict(name="external_local", theta=formula(external, 12, .6, .02)))
    spot.setValue(100.)
    risk.linkTo(ql.FlatForward(reference, .05, dc))
    dividend.linkTo(ql.FlatForward(reference, .02, dc))
    variance = ql.BlackVolTermStructureHandle(ql.BlackVarianceCurve(
        reference, [reference + 182, reference + 365], [.18, .26], dc
    ))
    variance_process = ql.GeneralizedBlackScholesProcess(
        ql.QuoteHandle(spot), dividend, risk, variance
    )
    observations.append(dict(name="linear_variance_curve",
                             theta=formula(variance_process, 12, .6, .02)))
    args.output.write_text(json.dumps(dict(
        source_revision=PIN, source_sha256=hashlib.sha256(source_bytes).hexdigest(),
        compiled_quantlib=ql.__version__, direct_helper_export=False,
        vanilla=rows, process_observations=observations,
    ), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
