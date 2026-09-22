"""Independent QuantLib 1.43 MC-American/Bermudan reference generator."""

import json

import QuantLib as ql

assert ql.__version__ == "1.43"
ql.Settings.instance().evaluationDate = ql.Date(15, 5, 1998)
settlement = ql.Date(17, 5, 1998)
maturity = ql.Date(17, 5, 1999)
dc = ql.Actual365Fixed()


def process(vol):
    return ql.BlackScholesMertonProcess(
        ql.QuoteHandle(ql.SimpleQuote(36)),
        ql.YieldTermStructureHandle(ql.FlatForward(settlement, 0.0, dc)),
        ql.YieldTermStructureHandle(ql.FlatForward(settlement, 0.06, dc)),
        ql.BlackVolTermStructureHandle(
            ql.BlackConstantVol(settlement, ql.NullCalendar(), vol, dc)
        ),
    )


rows = []
for strike in (36, 40):
    for vol in (0.2 + 0.1 * j for j in range(3)):
        p = process(vol)
        option = ql.VanillaOption(
            ql.PlainVanillaPayoff(ql.Option.Put, strike),
            ql.AmericanExercise(settlement, maturity),
        )
        option.setPricingEngine(
            ql.MCAmericanEngine(
                p,
                "pseudorandom",
                timeSteps=75,
                antitheticVariate=True,
                requiredTolerance=0.02,
                seed=42,
                polynomOrder=3,
            )
        )
        mc, error = option.NPV(), option.errorEstimate()
        option.setPricingEngine(ql.FdBlackScholesVanillaEngine(p, 401, 200))
        rows.append(dict(strike=strike, vol=vol, mc=mc, error=error, fd=option.NPV()))

bermudan = ql.VanillaOption(
    ql.PlainVanillaPayoff(ql.Option.Put, 40),
    ql.BermudanExercise([settlement + 91, settlement + 203, maturity]),
)
p = process(0.2)
bermudan.setPricingEngine(ql.FdBlackScholesVanillaEngine(p, 1600, 800))
fd = bermudan.NPV()
bermudan.setPricingEngine(
    ql.MCAmericanEngine(
        p,
        "pseudorandom",
        timeSteps=3,
        antitheticVariate=True,
        requiredSamples=32768,
        seed=42,
        polynomOrder=2,
        nCalibrationSamples=8192,
    )
)
bermudan_reference = dict(
    dates=[91, 203, 365],
    fd=fd,
    exercise_only_mc=bermudan.NPV(),
    error=bermudan.errorEstimate(),
)
bermudan.setPricingEngine(
    ql.MCAmericanEngine(
        p,
        "pseudorandom",
        timeSteps=75,
        antitheticVariate=True,
        requiredSamples=32768,
        seed=42,
        polynomOrder=2,
        nCalibrationSamples=8192,
    )
)
bermudan_reference["upstream_unmasked_refined_mc"] = bermudan.NPV()
bases = []
for family in (0, 1, 2, 3, 6):
    option = ql.VanillaOption(
        ql.PlainVanillaPayoff(ql.Option.Put, 40),
        ql.AmericanExercise(settlement, maturity),
    )
    option.setPricingEngine(
        ql.MCAmericanEngine(
            p,
            "pseudorandom",
            timeSteps=12,
            antitheticVariate=True,
            requiredSamples=4096,
            seed=42,
            polynomOrder=2,
            polynomType=family,
            nCalibrationSamples=2048,
        )
    )
    bases.append(dict(family=family, value=option.NPV(), error=option.errorEstimate()))
print(
    json.dumps(
        dict(american=rows, bermudan=bermudan_reference, bases=bases),
        indent=2,
        sort_keys=True,
    )
)
