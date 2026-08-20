"""Oracle for the instrument ergonomics surface (#882): calculate(),
is_calculated(), the per-facade one-shot price(arg) and the frozen results()
snapshot.

The surface layers over the core's lazy-object contract rather than replacing
it: set_*_engine still only wires the observer and marks the cache stale, and
the valuation still fires on the first accessor. What is new is that the
firing, the cache state and the copied outputs are all sayable from Python.

This file carries the VanillaOption pass. Two things it pins are worth naming.

A. price(process) is exactly set_engine(process) + npv(), same float, one call.
   The equality is bit-exact, not a tolerance: it is the same code path.

B. The evaluation-date arm pins the OBSERVER contract only, and deliberately
   NOT a price change. A VanillaOption registers with the settings evaluation
   date in its constructor (oneassetoption.rs:159), so moving that date does
   invalidate the cache. But BlackScholesProcess builds FIXED-reference curves
   (market.rs:86-100) and the analytic European engine reads every input off
   them by absolute date - black_variance_date, discount_date, reference_date
   (pricingengines/vanilla/mod.rs:105-133). The evaluation date therefore never
   enters this engine's price, and the recalculation reproduces the old NPV
   BIT FOR BIT. Asserting a moved price here would be asserting something
   false. The genuine frozen-copy discriminator, which needs an input the price
   really depends on, lives on CapFloor in this file's second pass.

   The moved date must also stay well inside the option's life. Past expiry the
   core short-circuits to setup_expired() and reports a zero NPV, and the arm
   would then pass for entirely the wrong reason.

Also pinned honestly rather than hopefully: the analytic engine fills neither
error_estimate nor valuation_date, so both read None on the snapshot. Its seven
extra outputs are all real-valued, so all seven survive the Real-only
additional_results copy, and the two that the fixture states outright - the
strike and the spot - are checked against the numbers the fixture was built
with, so a snapshot that copied the keys but garbled the values fails.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import IborLeg
from itofin.indexes import Estr, Euribor
from itofin.instruments import (
    CapFloor,
    CreditDefaultSwap,
    EuropeanExercise,
    MakeOis,
    OptionType,
    ProtectionSide,
    SettlementMethod,
    SettlementType,
    Swaption,
    SwapType,
    VanillaOption,
    VanillaSwap,
)
from itofin.pricingengines import (
    BlackCapFloorEngine,
    BlackSwaptionEngine,
    CashAnnuityModel,
    MidPointCdsEngine,
)
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, FlatHazardRate
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

REF = Date(15, 1, 2026)
EXPIRY = Date(15, 1, 2027)
MID_LIFE = Date(15, 6, 2026)

SPOT = 100.0
STRIKE = 100.0
RISK_FREE = 0.03
DIVIDEND = 0.01
VOL = 0.20


def _process():
    return BlackScholesProcess(
        SPOT, RISK_FREE, DIVIDEND, VOL, REF, DayCounter.actual365_fixed()
    )


def _option(settings):
    return VanillaOption(OptionType.Call, STRIKE, EXPIRY, settings)


def _settings():
    settings = Settings()
    settings.set_evaluation_date(REF)
    return settings


def test_price_is_set_engine_plus_npv():
    settings = _settings()
    one_shot = _option(settings)
    long_hand = _option(settings)
    long_hand.set_engine(_process())

    assert one_shot.price(_process()) == long_hand.npv()


def test_an_option_starts_uncalculated_and_latches_once_priced():
    settings = _settings()
    option = _option(settings)
    assert not option.is_calculated()

    option.price(_process())
    assert option.is_calculated()


def test_calculate_leaves_the_npv_reachable_and_is_idempotent():
    settings = _settings()
    option = _option(settings)
    option.set_engine(_process())

    option.calculate()
    priced = option.npv()
    option.calculate()

    assert option.is_calculated()
    assert option.npv() == priced


def test_the_snapshot_reports_the_npv_just_priced():
    settings = _settings()
    option = _option(settings)
    priced = option.price(_process())

    assert option.results().npv == priced


def test_the_analytic_engine_fills_neither_error_estimate_nor_valuation_date():
    """Asserted as the engine actually behaves, not as a richer engine would:
    pricingengines/vanilla/mod.rs sets results.value and the greeks and never
    touches the other two instrument-level fields."""
    settings = _settings()
    option = _option(settings)
    option.price(_process())
    snapshot = option.results()

    assert snapshot.error_estimate is None
    assert snapshot.valuation_date is None


def test_the_snapshot_carries_the_engines_real_valued_tags():
    """The seven tags the analytic engine writes are all reals, so all seven
    survive the Real-only copy. Two of them are inputs the fixture states, so
    they pin the values rather than only the keys."""
    settings = _settings()
    option = _option(settings)
    option.price(_process())
    extras = option.results().additional_results

    assert set(extras) == {
        "spot",
        "strike",
        "forward",
        "volatility",
        "timeToExpiry",
        "riskFreeDiscount",
        "dividendDiscount",
    }
    assert extras["spot"] == SPOT
    assert extras["strike"] == STRIKE
    assert extras["volatility"] == pytest.approx(VOL, abs=1e-12)


def test_an_evaluation_date_move_invalidates_the_cache():
    """The observer half of the lazy contract. See section B of the module
    docstring for why the recalculated price is UNCHANGED here, and why that is
    the correct assertion rather than a weak one."""
    settings = _settings()
    option = _option(settings)
    priced = option.price(_process())
    assert option.is_calculated()

    settings.set_evaluation_date(MID_LIFE)
    assert not option.is_calculated()

    reread = option.npv()
    assert option.is_calculated()
    assert reread == priced


def test_the_snapshot_survives_the_evaluation_date_move():
    settings = _settings()
    option = _option(settings)
    priced = option.price(_process())
    snapshot = option.results()

    settings.set_evaluation_date(MID_LIFE)

    assert snapshot.npv == priced


def test_the_snapshot_is_read_only():
    settings = _settings()
    option = _option(settings)
    option.price(_process())
    snapshot = option.results()

    with pytest.raises(AttributeError):
        snapshot.npv = 0.0


def test_calculating_without_an_engine_raises():
    settings = _settings()
    option = _option(settings)

    with pytest.raises(ItofinError) as raised:
        option.calculate()
    assert "null pricing engine" in str(raised.value)

    with pytest.raises(ItofinError):
        option.results()


CAP_EVAL = Date(14, 3, 2002)
CAP_REFERENCE = Date(18, 3, 2002)
CAP_RATE = 0.07
CAP_VOL = 0.20
DEARER_VOL = 0.25
CACHED_CAP = 6.87570026732


def _cap_fixture():
    """The #626 oracle cap: the 20Y semiannual Euribor6M leg of
    blackcapfloorengine.rs on a flat 5% Actual360 curve, capped at 7%.

    The volatility quote is handed back rather than dropped inside the engine
    builder, which is the whole point of this pass: it is the one input of this
    instrument that a Python name can still move after pricing."""
    settings = Settings()
    settings.set_evaluation_date(CAP_EVAL)
    calendar = Calendar.target()
    modified_following = BusinessDayConvention.ModifiedFollowing
    curve = FlatForward(CAP_REFERENCE, 0.05, DayCounter.actual360())
    index = Euribor.six_months(curve, settings)
    end = calendar.advance(CAP_REFERENCE, 20, "Years", modified_following, False)
    schedule = Schedule(
        CAP_REFERENCE,
        end,
        Frequency.Semiannual,
        calendar,
        modified_following,
        termination_convention=modified_following,
    )
    leg = (
        IborLeg(schedule, index)
        .with_notional(100.0)
        .with_payment_day_counter(index.day_counter())
        .with_payment_adjustment(modified_following)
        .with_fixing_days(2)
    )
    cap = CapFloor.cap(leg, [CAP_RATE], settings)
    quote = SimpleQuote(CAP_VOL)
    engine = BlackCapFloorEngine.with_flat_vol(
        curve, quote, DayCounter.actual365_fixed(), 0.0, settings
    )
    return cap, engine, quote


def test_cap_price_is_set_black_engine_plus_npv():
    one_shot, engine, _ = _cap_fixture()
    long_hand, other_engine, _ = _cap_fixture()
    long_hand.set_black_engine(other_engine)

    assert one_shot.price(engine) == long_hand.npv()


def test_the_snapshot_freezes_while_the_live_cap_reprices():
    """THE frozen-copy discriminator, and the reason it lives on the cap rather
    than the option: this instrument has an input a Python name can still move
    after pricing. SimpleQuote.set_value is live through the shared inner
    (market.rs:43-52), the constant surface with_flat_vol builds registers with
    that quote (constantoptionletvol.rs:54), the Black engine registers with the
    surface (blackcapfloorengine.rs:88-89), and the instrument registers with
    its engine (instrument.rs:157-159). So the quote move genuinely reaches the
    cap's lazy core.

    A live-view snapshot would move with the instrument and this would fail. A
    frozen copy keeps the value it was taken from, which is what is asserted -
    alongside the live accessor moving, so the arm cannot pass by the price
    simply never changing."""
    cap, engine, quote = _cap_fixture()
    priced = cap.price(engine)
    snapshot = cap.results()
    assert abs(priced - CACHED_CAP) <= 1e-11
    assert snapshot.npv == priced
    assert cap.is_calculated()

    quote.set_value(DEARER_VOL)
    assert not cap.is_calculated()

    reprised = cap.npv()
    print(f"\npriced@{CAP_VOL} = {priced!r}")
    print(f"snapshot after the move = {snapshot.npv!r}")
    print(f"live npv@{DEARER_VOL} = {reprised!r}")
    assert reprised > priced
    assert snapshot.npv == priced


def test_a_cap_snapshot_taken_after_the_move_reads_the_new_price():
    """The converse of the arm above: results() calculates first, so a snapshot
    taken after the quote moved reports the repriced value, not the stale one.
    Without this, a results() that silently skipped the calculation would still
    pass the freeze arm."""
    cap, engine, quote = _cap_fixture()
    before = cap.price(engine)

    quote.set_value(DEARER_VOL)
    after = cap.results()

    assert after.npv > before
    assert after.npv == cap.npv()


def test_capfloor_calculating_without_an_engine_raises():
    cap, _, _ = _cap_fixture()

    with pytest.raises(ItofinError) as raised:
        cap.calculate()
    assert "null pricing engine" in str(raised.value)


SWAPTION_EVAL = Date(15, 1, 2026)
SWAPTION_EXERCISE = Date(15, 1, 2027)
SWAPTION_START = Date(15, 1, 2028)
SWAPTION_END = Date(15, 1, 2033)
SWAPTION_STRIKE = 0.03
SWAPTION_VOL = 0.20


def _swaption_fixture():
    """The #612 swaption fixture: a 1Y-into-5Y payer at 3% on a flat 3% curve,
    priced off a flat 20% constant swaption surface. One Settings object drives
    the curve, the swap, the swaption and the engine."""
    settings = Settings()
    settings.set_evaluation_date(SWAPTION_EVAL)
    unadjusted = BusinessDayConvention.Unadjusted
    curve = FlatForward(SWAPTION_EVAL, 0.03, DayCounter.actual365_fixed())
    fixed = Schedule(
        SWAPTION_START, SWAPTION_END, Frequency.Annual, Calendar.target(), unadjusted
    )
    floating = Schedule(
        SWAPTION_START, SWAPTION_END, Frequency.Semiannual, Calendar.target(), unadjusted
    )
    swap = VanillaSwap(
        SwapType.Payer,
        100.0,
        fixed,
        SWAPTION_STRIKE,
        DayCounter.thirty360_bond_basis(),
        floating,
        Euribor.six_months(curve, settings),
        0.0,
        DayCounter.actual360(),
        settings,
    )
    swaption = Swaption(
        swap,
        EuropeanExercise(SWAPTION_EXERCISE),
        SettlementType.Physical,
        SettlementMethod.PhysicalOTC,
        settings,
    )
    engine = BlackSwaptionEngine.with_flat_vol(
        curve,
        SimpleQuote(SWAPTION_VOL),
        DayCounter.actual365_fixed(),
        0.0,
        settings,
        CashAnnuityModel.SwapRate,
    )
    return swaption, engine


def test_swaption_price_is_set_black_engine_plus_npv():
    """Each arm builds its own swaption: the Black engine silently installs a
    discounting engine on the swap it prices, so a shared one would let this
    pass on a stale cached number."""
    one_shot, engine = _swaption_fixture()
    long_hand, other_engine = _swaption_fixture()
    long_hand.set_black_engine(other_engine)

    priced = one_shot.price(engine)
    print(f"\nswaption price = {priced!r}")
    assert priced == long_hand.npv()
    assert priced > 0.0


def test_the_swaption_snapshot_reports_the_priced_npv():
    swaption, engine = _swaption_fixture()
    assert not swaption.is_calculated()
    priced = swaption.price(engine)

    assert swaption.is_calculated()
    assert swaption.results().npv == priced


def test_swaption_calculating_without_an_engine_raises():
    swaption, _ = _swaption_fixture()

    with pytest.raises(ItofinError) as raised:
        swaption.calculate()
    assert "null pricing engine" in str(raised.value)


SWAP_START = Date(15, 1, 2028)
SWAP_END = Date(15, 1, 2033)


def _vanilla_swap_fixture():
    """The #500 swap: a 5Y payer at 3% on a flat 3% curve. Its engine takes two
    arguments rather than an engine object, which is why this facade's price()
    takes the same two."""
    settings = Settings()
    settings.set_evaluation_date(SWAPTION_EVAL)
    modified_following = BusinessDayConvention.ModifiedFollowing
    curve = FlatForward(SWAPTION_EVAL, 0.03, DayCounter.actual365_fixed())
    fixed = Schedule(
        SWAP_START, SWAP_END, Frequency.Annual, Calendar.target(), modified_following
    )
    floating = Schedule(
        SWAP_START, SWAP_END, Frequency.Semiannual, Calendar.target(), modified_following
    )
    swap = VanillaSwap(
        SwapType.Payer,
        100.0,
        fixed,
        0.03,
        DayCounter.thirty360_bond_basis(),
        floating,
        Euribor.six_months(curve, settings),
        0.0,
        DayCounter.actual360(),
        settings,
    )
    return swap, curve, settings


def test_vanilla_swap_price_is_set_engine_plus_npv():
    one_shot, curve, settings = _vanilla_swap_fixture()
    long_hand, other_curve, other_settings = _vanilla_swap_fixture()
    long_hand.set_engine(other_curve, other_settings)

    assert one_shot.price(curve, settings) == long_hand.npv()


def test_the_vanilla_swap_snapshot_reports_the_priced_npv():
    swap, curve, settings = _vanilla_swap_fixture()
    assert not swap.is_calculated()
    priced = swap.price(curve, settings)

    assert swap.is_calculated()
    assert swap.results().npv == priced


def test_vanilla_swap_calculating_without_an_engine_raises():
    swap, _, _ = _vanilla_swap_fixture()

    with pytest.raises(ItofinError) as raised:
        swap.calculate()
    assert "null pricing engine" in str(raised.value)


def _ois_fixture(fixed_rate=0.03):
    """A 5Y ESTR OIS off a flat 3% curve. MakeOis.build attaches the discounting
    engine itself, which is why this is the one facade whose price() takes no
    argument at all."""
    settings = Settings()
    settings.set_evaluation_date(SWAPTION_EVAL)
    curve = FlatForward(SWAPTION_EVAL, 0.03, DayCounter.actual365_fixed())
    return MakeOis(
        Period(5, "Years"),
        Estr(curve, settings),
        settings,
        fixed_rate=fixed_rate,
        nominal=100.0,
    ).build()


def test_the_no_argument_ois_price_is_its_npv():
    """The engine arrived with the instrument, so price() and npv() can only
    differ by the forced calculate() - which is exactly the point: price()
    reads the same on this facade as on the eight that install an engine."""
    swap = _ois_fixture()
    priced = swap.price()

    assert swap.is_calculated()
    assert priced == swap.npv()
    assert priced == swap.results().npv


def test_the_ois_price_moves_with_its_fixed_rate():
    """Guards the no-argument price() against reporting a constant: a swap paying
    3% and one paying 5% off the same 3% curve are not worth the same."""
    at_market = _ois_fixture(0.03)
    off_market = _ois_fixture(0.05)

    print(f"\nOIS @3% = {at_market.price()!r}\nOIS @5% = {off_market.price()!r}")
    assert at_market.price() != off_market.price()


CDS_TODAY = Date(9, 6, 2006)
CDS_ISSUE = Date(9, 6, 2005)
CDS_MATURITY = Date(9, 6, 2015)
CACHED_CDS_NPV = 295.0153398


def _cds_fixture():
    """The #739 contract: ten years of protection sold on a flat 1.234% hazard
    rate, discounted at a flat 6%, worth 295.0153398."""
    settings = Settings()
    settings.set_evaluation_date(CDS_TODAY)
    calendar = Calendar.target()
    day_counter = DayCounter.actual360()
    convention = BusinessDayConvention.ModifiedFollowing
    hazard = FlatHazardRate.moving(
        0, calendar, SimpleQuote(0.01234), day_counter, settings
    )
    discount = FlatForward(CDS_TODAY, 0.06, day_counter)
    schedule = Schedule(
        CDS_ISSUE, CDS_MATURITY, Frequency.Semiannual, calendar, convention
    )
    contract = CreditDefaultSwap(
        ProtectionSide.Seller,
        10000.0,
        0.0120,
        schedule,
        convention,
        day_counter,
        True,
        True,
        settings,
    )
    return contract, MidPointCdsEngine(hazard, 0.4, discount, settings)


def test_cds_price_is_set_engine_plus_npv_and_hits_the_cached_value():
    one_shot, engine = _cds_fixture()
    long_hand, other_engine = _cds_fixture()
    long_hand.set_engine(other_engine)

    priced = one_shot.price(engine)
    assert priced == long_hand.npv()
    assert abs(priced - CACHED_CDS_NPV) <= 1e-7


def test_the_cds_snapshot_reports_the_priced_npv():
    contract, engine = _cds_fixture()
    assert not contract.is_calculated()
    priced = contract.price(engine)

    assert contract.is_calculated()
    assert contract.results().npv == priced


def test_cds_calculating_without_an_engine_raises():
    contract, _ = _cds_fixture()

    with pytest.raises(ItofinError) as raised:
        contract.calculate()
    assert "null pricing engine" in str(raised.value)
