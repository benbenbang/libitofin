"""Oracle for the MakeVanillaSwap facade (issue #531).

The builder DERIVES what ``test_vanilla_swap.py`` states by hand, so the two
fixtures are the oracle for each other. Feeding
``MakeVanillaSwap(5Y, Euribor6M, settings, effective_date=15-Jan-2028,
nominal=100)`` reproduces that file's swap only if every derivation is right:

* the end date, ``start + 5Y`` -> 15-Jan-2033 (makevanillaswap.rs:472-484);
* the EUR fixed-leg defaults, tenor 1Y and Thirty360(BondBasis)
  (makevanillaswap.rs:531-551), which the hand-built swap spells out as
  ``Frequency.Annual`` + ``DayCounter.thirty360_bond_basis()``;
* the floating leg taken from the index, 6M on Actual360
  (makevanillaswap.rs:336-337);
* both schedules on the index fixing calendar (TARGET) under
  ModifiedFollowing (makevanillaswap.rs:339-362).

The builder generates its schedules ``Backward`` while ``Schedule`` generates
them ``forwards`` (time.rs:413); the fixture spans a whole number of annual and
semiannual periods, so the two agree date for date and the comparison stays
apples-to-apples.

``PROBE_FAIR`` / ``PROBE_NPV`` are ``test_vanilla_swap.py``'s independently
pinned Rust-probe constants for the same fixture at a 3% fixed rate.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import MakeVanillaSwap, SwapType, VanillaSwap
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

PROBE_FAIR = 0.03048844643136293
PROBE_NPV = 0.21033895380698553

REF = Date(15, 1, 2026)
START = Date(15, 1, 2028)
END = Date(15, 1, 2033)


def _fixture():
    settings = Settings()
    settings.set_evaluation_date(REF)
    curve = FlatForward(REF, 0.03, DayCounter.actual365_fixed())
    index = Euribor.six_months(curve, settings)
    return settings, curve, index


def _built(fixed_rate):
    settings, _, index = _fixture()
    return MakeVanillaSwap(
        Period(5, "Years"),
        index,
        settings,
        fixed_rate=fixed_rate,
        effective_date=START,
        nominal=100.0,
    ).build()


def _hand_built(fixed_rate):
    settings, curve, index = _fixture()
    fixed = Schedule(
        START,
        END,
        Frequency.Annual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    floating = Schedule(
        START,
        END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    swap = VanillaSwap(
        SwapType.Payer,
        100.0,
        fixed,
        fixed_rate,
        DayCounter.thirty360_bond_basis(),
        floating,
        index,
        0.0,
        DayCounter.actual360(),
        settings,
    )
    swap.set_engine(curve, settings)
    return swap


def test_par_swap_prices_to_zero():
    """fixed_rate=None fills the leg with the fair rate, so the NPV vanishes."""
    swap = _built(None)
    assert swap.npv() == pytest.approx(0.0, abs=1e-10)


def test_par_swap_fair_rate_matches_hand_built_swap():
    """The derived swap and the stated one agree on the fair rate, and the par
    fill wrote exactly that rate into the fixed leg."""
    built = _built(None)
    hand = _hand_built(0.03)
    assert built.fair_rate() == pytest.approx(hand.fair_rate(), abs=1e-12)
    assert built.fair_rate() == pytest.approx(PROBE_FAIR, abs=1e-12)
    assert built.fixed_rate() == pytest.approx(PROBE_FAIR, abs=1e-12)


def test_built_swap_reproduces_the_hand_built_npv():
    """At the same 3% fixed rate the derived swap prices to the hand-built NPV,
    which pins the schedules, the fixed-leg defaults and the engine."""
    built = _built(0.03)
    assert built.npv() == pytest.approx(_hand_built(0.03).npv(), abs=1e-12)
    assert built.npv() == pytest.approx(PROBE_NPV, abs=1e-10)
    assert built.nominal() == pytest.approx(100.0, abs=1e-12)


def test_npv_sign_flips_around_the_fair_rate():
    """Payer NPV is positive below the fair rate and negative above it."""
    assert _built(0.02).npv() > 0.0
    assert _built(0.06).npv() < 0.0


def test_fixed_leg_overrides_change_the_price():
    """A semiannual Actual360 fixed leg is a different swap from the EUR
    defaults, so its fair rate must differ."""
    settings, _, index = _fixture()
    overridden = MakeVanillaSwap(
        Period(5, "Years"),
        index,
        settings,
        effective_date=START,
        nominal=100.0,
        fixed_leg_tenor=Period(6, "Months"),
        fixed_leg_day_count=DayCounter.actual360(),
    ).build()
    assert overridden.fair_rate() != pytest.approx(PROBE_FAIR, abs=1e-6)


def test_derived_start_date_needs_an_evaluation_date():
    """Without an effective date the start is derived from the evaluation date;
    an unset one is an ItofinError, not a clock fallback (D10)."""
    settings = Settings()
    settings.set_evaluation_date(REF)
    curve = FlatForward(REF, 0.03, DayCounter.actual365_fixed())
    index = Euribor.six_months(curve, Settings())
    with pytest.raises(ItofinError):
        MakeVanillaSwap(Period(5, "Years"), index, Settings()).build()
