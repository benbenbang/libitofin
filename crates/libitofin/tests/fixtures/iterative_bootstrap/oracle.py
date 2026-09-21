"""Independent QuantLib 1.43 iterative-bootstrap failure and recovery oracle."""

import QuantLib as q

assert q.__version__ == "1.43", q.__version__

q.Settings.instance().evaluationDate = q.Date(15, 6, 2026)


def build(rate, **options):
    quote = q.SimpleQuote(rate)
    helper = q.DepositRateHelper(
        q.QuoteHandle(quote), q.Period(1, q.Years), 0, q.NullCalendar(),
        q.Unadjusted, False, q.Actual365Fixed(),
    )
    curve = q.PiecewiseLinearZero(
        q.Date(15, 6, 2026), [helper], q.Actual365Fixed(),
        q.IterativeBootstrap(**options),
    )
    return curve, quote


cases = [
    ("positive_min", .015, dict(minValue=.04, maxValue=.1, maxAttempts=3)),
    ("negative_max", -.015, dict(minValue=-.1, maxValue=-.04, maxAttempts=3)),
    ("positive", .25, dict(minValue=.01, maxValue=.1, maxAttempts=3)),
    ("negative", -.25, dict(minValue=-.1, maxValue=-.01, maxAttempts=3)),
    ("fallback_upper", .25, dict(minValue=.01, maxValue=.1, dontThrow=True)),
    ("fallback_lower", -.25, dict(minValue=-.1, maxValue=-.01, dontThrow=True)),
    ("eval_fallback", .25, dict(minValue=.01, maxValue=.4, maxEvaluations=1,
                              dontThrow=True, dontThrowSteps=10)),
]
for name, rate, options in cases:
    curve, quote = build(rate, **options)
    print(name, curve.data(), curve.discount(1))

curve, quote = build(.01)
print("cached_before", curve.data())
quote.setValue(.4)
print("cached_after", curve.data())
for name, rate, options in cases[:4]:
    try:
        print(name, build(rate, **dict(options, maxAttempts=2))[0].data())
    except RuntimeError as error:
        print(name, "FAIL", str(error)[:100])

calendar = q.TARGET()
reference = calendar.advance(q.Settings.instance().evaluationDate, 2, q.Days)
helpers = [
    q.DepositRateHelper(rate, q.Euribor(q.Period(n, unit)))
    for n, unit, rate in [(1, q.Weeks, .04559), (1, q.Months, .04581),
                          (3, q.Months, .04557)]
]
helpers += [
    q.FuturesRateHelper(95.5, q.IMM.nextDate(reference, False), 3, calendar,
                        q.ModifiedFollowing, False, q.Actual360()),
    q.FraRateHelper(.046, 9, q.Euribor6M()),
]
helpers += [
    q.SwapRateHelper(rate, q.Period(n, q.Years), calendar, q.Annual, q.Unadjusted,
                    q.Thirty360(q.Thirty360.BondBasis), q.Euribor6M())
    for n, rate in [(2, .0463), (3, .0475), (5, .0499)]
]
for fallback in [False, True]:
    curve = q.PiecewiseCubicZero(
        reference, helpers, q.Actual360(),
        q.IterativeBootstrap(accuracy=1e-20, dontThrow=fallback),
    )
    try:
        print("iteration_limit", fallback, curve.data())
    except RuntimeError as error:
        print("iteration_limit", fallback, str(error))
