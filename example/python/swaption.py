"""Build a vanilla payer swaption and an Eonia OIS swaption, then price with Black.

MakeSwaption supports vanilla SwapIndex underlyings and always builds a payer.
Overnight swaps use MakeOis followed by Swaption.from_ois instead.

The ATM fixing is pinned by sdk/go/testdata/makeswaption_oracle.json (QuantLib
1.43). The OIS price is QuantLib test-suite/swaption.cpp testCachedValue;
both checks retain an absolute tolerance of 1e-12. The 4% forwarding curve is
equivalent to that fixture's 5% continuous curve plus a -1% zero spread.

Run from the repository root: python example/python/swaption.py
"""

# standard library
import math

# itofin library
from itofin import Settings
from itofin.indexes import Currency, Eonia, Euribor, SwapIndex
from itofin.instruments import EuropeanExercise, MakeOis, MakeSwaption, SettlementMethod, SettlementType, Swaption
from itofin.pricingengines import BlackSwaptionEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Period


def check_close(label, actual, expected):
    """Reject numerical drift without relying on optional Python assertions."""
    if not math.isclose(actual, expected, rel_tol=0, abs_tol=1e-12):
        raise ValueError(f"{label}: got {actual:.16g}, expected {expected:.16g}")


def vanilla_swaption():
    """Use the builder's default ATM strike and TARGET exercise calendar."""
    settings = Settings()
    today = Date(9, 10, 2015)
    settings.set_evaluation_date(today)
    dc = DayCounter.actual360()
    curve = FlatForward(today, 0.05, dc)
    index = SwapIndex(
        "EuriborSwapIsdaFixA",
        Period(5, "Years"),
        2,
        Currency.eur(),
        Calendar.target(),
        Period(1, "Years"),
        BDC.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        Euribor.six_months(curve, settings),
        settings,
    )
    option = MakeSwaption(index, Period(1, "Years")).build()
    if option.exercise_date() != Date(10, 10, 2016):
        raise ValueError("unexpected vanilla exercise date")
    strike = option.underlying_fixed_rate()
    check_close("vanilla ATM strike", strike, 0.05202914613654939)
    engine = BlackSwaptionEngine.with_flat_vol(curve, SimpleQuote(0.20), dc, 0.0, settings)
    value = option.price(engine)
    print(f"Vanilla exercise=2016-10-10 ATM={strike:.12f} Black NPV={value:.12f}")


def overnight_swaption():
    """Retain the OIS underlying and observe live volatility changes."""
    settings = Settings()
    today = Date(13, 3, 2002)
    settings.set_evaluation_date(today)
    calendar = Calendar.target()
    dc = DayCounter.actual365_fixed()
    settlement = calendar.advance(today, 2, "Days", BDC.Following, False)
    discount = FlatForward(settlement, 0.05, dc)
    index = Eonia(FlatForward(settlement, 0.04, dc), settings)
    exercise = calendar.advance(settlement, 5, "Years", BDC.Following, False)
    start = calendar.advance(exercise, 2, "Days", BDC.Following, False)
    swap = MakeOis(
        Period(10, "Years"),
        index,
        settings,
        fixed_rate=0.06,
        effective_date=start,
        fixed_leg_day_count=DayCounter.thirty360_bond_basis(),
    ).build()
    option = Swaption.from_ois(
        swap,
        EuropeanExercise(exercise),
        SettlementType.Physical,
        SettlementMethod.PhysicalOTC,
        settings,
    )
    vol = SimpleQuote(0.20)
    engine = BlackSwaptionEngine.with_flat_vol(discount, vol, dc, 0.0, settings)
    value = option.price(engine)
    check_close("Eonia OIS Black NPV", value, 0.014101075767)
    del swap, index, engine
    vol.set_value(0.30)
    repriced = option.npv()
    if not repriced > value:
        raise ValueError("the volatility increase did not increase the OIS swaption value")
    print(f"Eonia OIS Black NPV={value:.12f}; volatility 20% -> 30%: {repriced:.12f}")


if __name__ == "__main__":
    vanilla_swaption()
    overnight_swaption()
