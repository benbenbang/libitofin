# Hand-written stubs for itofin.instruments; sync manually with src/option.rs,
# src/swap.rs, src/ois.rs, src/swaption.rs, src/capfloor.rs, src/credit.rs and
# src/inflation.rs (#517).

from itofin import Settings
from itofin.cashflows import IborLeg, YoYInflationCoupon
from itofin.indexes import (
    CpiInterpolationType,
    IborIndex,
    OvernightIndex,
    YoYInflationIndex,
    ZeroInflationIndex,
)
from itofin.models import HestonModel, HullWhite
from itofin.pricingengines import (
    BachelierSwaptionEngine,
    BlackCapFloorEngine,
    BlackSwaptionEngine,
    DiscountingSwapEngine,
    IsdaCdsEngine,
    MCAmericanEngine,
    MCEuropeanEngine,
    MCEuropeanHestonEngine,
    MidPointCdsEngine,
    YoYInflationCapFloorEngine,
)
from itofin.processes import BlackScholesProcess
from itofin.results import Results
from itofin.termstructures import RateAveraging, YieldTermStructure
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Period,
    Schedule,
)

class OptionType:
    """The call/put flag.

    A fieldless enum mirroring the core option type; the signed discriminant
    convention behind the two variants stays in the core.
    """

    Call: OptionType
    Put: OptionType

class VanillaOption:
    """A single-asset vanilla option: European by construction, American through american().

    Valuation is lazy: an accessor reprices only once an observed input - the
    attached engine, or the evaluation date on the Settings the option
    registered with - has notified it.
    """

    def __init__(
        self, option_type: OptionType, strike: float, expiry: Date, settings: Settings
    ) -> None:
        """Build the European-exercise option, exercisable only at expiry.

        Args:
            option_type (OptionType): Whether the payoff is a call or a put.
            strike (float): The strike of the plain vanilla payoff.
            expiry (Date): The single date the option may be exercised on.
            settings (Settings): The explicit settings supplying the evaluation
                date the option prices against.
        """
        ...
    @classmethod
    def american(
        cls,
        option_type: OptionType,
        strike: float,
        earliest: Date,
        latest: Date,
        settings: Settings,
    ) -> VanillaOption:
        """Build the option exercisable at any time over [earliest, latest].

        The option pays on exercise rather than at expiry. This is the exercise
        the Monte Carlo American engine requires; the analytic European engine
        rejects it.

        Args:
            option_type (OptionType): Whether the payoff is a call or a put.
            strike (float): The strike of the plain vanilla payoff.
            earliest (Date): The first date the option may be exercised on.
            latest (Date): The last date the option may be exercised on.
            settings (Settings): The explicit settings supplying the evaluation
                date the option prices against.

        Returns:
            VanillaOption: The American-exercise option.

        Raises:
            ItofinError: If earliest is after latest.
        """
        ...
    def set_engine(self, process: BlackScholesProcess) -> None:
        """Attach an analytic European engine built on process.

        Args:
            process (BlackScholesProcess): The process the engine prices on;
                the exact object this Python instance holds is threaded in.
        """
        ...
    def set_heston_engine(self, model: HestonModel, integration_order: int) -> None:
        """Attach an analytic Heston engine built on model.

        The analytic Heston engine fills only the value, so npv() works but the
        greeks raise on this path.

        Args:
            model (HestonModel): The calibrated Heston model to price under.
            integration_order (int): The order of the Gauss-Laguerre
                integration.

        Raises:
            ItofinError: If integration_order exceeds 192.
        """
        ...
    def set_mc_engine(self, engine: MCEuropeanEngine) -> None:
        """Attach the Monte Carlo European engine.

        Args:
            engine (MCEuropeanEngine): The engine, which already holds the
                process it prices on.
        """
        ...
    def set_mc_heston_engine(self, engine: MCEuropeanHestonEngine) -> None:
        """Attach the Monte Carlo Heston engine.

        Args:
            engine (MCEuropeanHestonEngine): The engine, which already holds
                the Heston process it prices on.
        """
        ...
    def set_mc_american_engine(self, engine: MCAmericanEngine) -> None:
        """Attach the Monte Carlo American engine.

        The option must have been built through american(): a European-exercise
        option raises ItofinError ("wrong exercise given") from npv().

        Args:
            engine (MCAmericanEngine): The engine, which already holds the
                process it prices on.
        """
        ...
    def calculate(self) -> None:
        """Force the valuation, so a later accessor reads a warm cache.

        Idempotent: the core short-circuits on a valid cache, and the option
        reprices only once an observed input notified it.

        Raises:
            ItofinError: If no engine is attached, no evaluation date is set,
                or the attached engine refuses the option.
        """
        ...
    def is_calculated(self) -> bool:
        """Return whether the cached results are currently valid.

        Returns:
            bool: True when the next accessor reads the cache rather than
                repricing.
        """
        ...
    def price(self, process: BlackScholesProcess) -> float:
        """Attach an analytic European engine on process and return the NPV.

        The one-shot form of set_engine followed by npv. The other engines keep
        their own setters and compose with calculate and npv as before.

        Args:
            process (BlackScholesProcess): The process the engine prices on.

        Returns:
            float: The present value under the analytic European engine.

        Raises:
            ItofinError: If no evaluation date is set or the engine refuses the
                option.
        """
        ...
    def results(self) -> Results:
        """Return a frozen snapshot of the valuation, calculating first.

        The snapshot does not track the option: once taken, an evaluation-date
        or engine change reprices the live accessors and leaves it alone.

        Returns:
            Results: A copy of the valuation results.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def npv(self) -> float:
        """Return the present value.

        Returns:
            float: The option value under the attached engine.

        Raises:
            ItofinError: If no evaluation date or no engine is set.
        """
        ...
    def delta(self) -> float:
        """Return the option delta.

        Returns:
            float: The sensitivity to the underlying spot.

        Raises:
            ItofinError: If the attached engine does not provide it, which the
                analytic Heston engine does not.
        """
        ...
    def gamma(self) -> float:
        """Return the option gamma.

        Returns:
            float: The second-order sensitivity to the underlying spot.

        Raises:
            ItofinError: If the attached engine does not provide it.
        """
        ...
    def theta(self) -> float:
        """Return the option theta.

        Returns:
            float: The sensitivity to the passage of time.

        Raises:
            ItofinError: If the attached engine does not provide it.
        """
        ...
    def vega(self) -> float:
        """Return the option vega.

        Returns:
            float: The sensitivity to the volatility.

        Raises:
            ItofinError: If the attached engine does not provide it.
        """
        ...
    def rho(self) -> float:
        """Return the option rho.

        Returns:
            float: The sensitivity to the risk-free rate.

        Raises:
            ItofinError: If the attached engine does not provide it.
        """
        ...
    def dividend_rho(self) -> float:
        """Return the option dividend rho.

        Returns:
            float: The sensitivity to the dividend yield.

        Raises:
            ItofinError: If the attached engine does not provide it.
        """
        ...
    def error_estimate(self) -> float:
        """Return the standard error on the present value.

        Returns:
            float: The Monte Carlo standard error.

        Raises:
            ItofinError: On the engines that do not produce one, which is every
                analytic engine here.
        """
        ...
    def exercise_probability(self) -> float:
        """Return the fraction of simulated paths exercised before expiry.

        Returns:
            float: The exercise probability reported by the engine.

        Raises:
            ItofinError: On every engine that does not report it - only
                MCAmericanEngine does.
        """
        ...

class SwapType:
    """Which side of the named leg the swap is seen from.

    A fieldless enum; the signed leg multiplier the two variants stand for
    stays in the core.
    """

    Payer: SwapType
    Receiver: SwapType

class VanillaSwap:
    """A fixed-vs-Ibor interest-rate swap.

    Pricing needs an engine: call set_engine before fair_rate or npv.
    """

    def __init__(
        self,
        swap_type: SwapType,
        nominal: float,
        fixed_schedule: Schedule,
        fixed_rate: float,
        fixed_day_count: DayCounter,
        float_schedule: Schedule,
        ibor_index: IborIndex,
        spread: float,
        floating_day_count: DayCounter,
        settings: Settings,
    ) -> None:
        """Build the swap from both schedules spelled out.

        Args:
            swap_type (SwapType): Whether the fixed leg is paid or received.
            nominal (float): The notional both legs accrue on.
            fixed_schedule (Schedule): The fixed leg's payment schedule.
            fixed_rate (float): The rate the fixed leg accrues at.
            fixed_day_count (DayCounter): The day count of the fixed leg.
            float_schedule (Schedule): The floating leg's payment schedule.
            ibor_index (IborIndex): The index the floating leg fixes off.
            spread (float): The spread added to every floating fixing.
            floating_day_count (DayCounter): The day count of the floating leg.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Raises:
            ItofinError: If the floating leg cannot be built, a degenerate leg
                being the usual cause.
        """
        ...
    def set_engine(self, curve: YieldTermStructure, settings: Settings) -> None:
        """Attach a discounting engine over curve so the swap prices.

        The engine is built with the settings-driven flow defaults, leaving the
        settlement date, the NPV date and the settlement-date-flows flag unset.

        Args:
            curve (YieldTermStructure): The curve the flows discount on.
            settings (Settings): The settings the engine resolves its dates
                against.
        """
        ...
    def calculate(self) -> None:
        """Force the valuation. Idempotent.

        Raises:
            ItofinError: If no engine is attached, no evaluation date is set,
                or the attached engine refuses the swap.
        """
        ...
    def is_calculated(self) -> bool:
        """Return whether the cached results are currently valid.

        Returns:
            bool: True when the next accessor reads the cache.
        """
        ...
    def price(self, curve: YieldTermStructure, settings: Settings) -> float:
        """Attach a discounting engine over curve and return the NPV.

        set_engine followed by npv, in one call, and it takes the same two
        arguments for the same reason.

        Args:
            curve (YieldTermStructure): The curve the flows discount on.
            settings (Settings): The settings the engine resolves its dates
                against.

        Returns:
            float: The swap value under the freshly built engine.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def results(self) -> Results:
        """Return a frozen snapshot of the valuation, calculating first.

        Returns:
            Results: A copy of the valuation results.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def fair_rate(self) -> float:
        """Return the fixed rate that zeroes the swap NPV.

        Returns:
            float: The fair fixed rate.

        Raises:
            ItofinError: If no engine is attached or the swap has expired.
        """
        ...
    def npv(self) -> float:
        """Return the swap NPV under the attached engine.

        Returns:
            float: The present value.

        Raises:
            ItofinError: If no engine is attached.
        """
        ...
    def nominal(self) -> float:
        """Return the notional both legs accrue on.

        Returns:
            float: The single nominal.

        Raises:
            ItofinError: If the legs carry per-coupon nominals, which leaves no
                single one to report.
        """
        ...
    def fixed_rate(self) -> float:
        """Return the fixed-leg rate.

        Returns:
            float: The rate the fixed leg accrues at.
        """
        ...

class MakeVanillaSwap:
    """Market-convention builder for a VanillaSwap.

    Derives the start and end dates, both schedules, the fixed-leg tenor and
    day count and the discounting engine from a swap tenor and an Ibor index,
    so the caller states conventions instead of hand-building two schedules.
    ``fixed_rate=None`` builds a par swap: the fair rate is computed and written
    into the fixed leg, so the result prices to a zero NPV.

    The core builder is a consumed-self fluent chain, which does not cross the
    FFI boundary; this facade takes the overrides as constructor keywords and
    assembles the chain inside build(). Only four overrides are exposed; every
    other core one keeps its default, so the discounting curve is always the
    index's forwarding curve. The built swap already carries its
    DiscountingSwapEngine.
    """

    def __init__(
        self,
        swap_tenor: Period,
        ibor_index: IborIndex,
        settings: Settings,
        fixed_rate: float | None = None,
        forward_start: Period | None = None,
        effective_date: Date | None = None,
        nominal: float | None = None,
        fixed_leg_tenor: Period | None = None,
        fixed_leg_day_count: DayCounter | None = None,
    ) -> None:
        """Store the configuration the chain is assembled from in build().

        Args:
            swap_tenor (Period): The length of the swap.
            ibor_index (IborIndex): The index the floating leg fixes off, and
                whose forwarding curve discounts.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
            fixed_rate (float | None): The rate of the fixed leg; None builds a
                par swap.
            forward_start (Period | None): The delay before the swap starts;
                None starts it spot, at a zero-day period.
            effective_date (Date | None): The start date; None derives it from
                the evaluation date.
            nominal (float | None): The notional; None keeps the core default.
            fixed_leg_tenor (Period | None): The fixed-leg payment tenor; None
                takes the currency's market convention.
            fixed_leg_day_count (DayCounter | None): The fixed-leg day count;
                None takes the currency's market convention.
        """
        ...
    def build(self) -> VanillaSwap:
        """Build the priced swap.

        Returns:
            VanillaSwap: The swap, already carrying its discounting engine.

        Raises:
            ItofinError: If effective_date is unset and no evaluation date is
                set to derive the start from; if the index is neither EUR nor
                USD, the two the fixed-leg defaults are known for; or if the
                par-rate fill fails to price.
        """
        ...

class OvernightIndexedSwap:
    """A fixed leg versus a compounded overnight leg.

    Only MakeOis builds one, so it always arrives priced; there is no
    set_engine and no raw constructor (both deferred with the two-schedule
    master ctor)."""

    def fair_rate(self) -> float:
        """Return the fixed rate that zeroes the swap NPV.

        Returns:
            float: The fair fixed rate, read through the swap's base.

        Raises:
            ItofinError: If the swap has expired or its engine fails to price.
        """
        ...
    def calculate(self) -> None:
        """Force the valuation. Idempotent.

        Raises:
            ItofinError: If no evaluation date is set or the engine refuses the
                swap.
        """
        ...
    def is_calculated(self) -> bool:
        """Return whether the cached results are currently valid.

        Returns:
            bool: True when the next accessor reads the cache.
        """
        ...
    def price(self) -> float:
        """Price the swap and return the NPV.

        The only no-argument price(): MakeOis already attached the discounting
        engine, so none is left to install.

        Returns:
            float: The present value.

        Raises:
            ItofinError: On anything that makes the valuation fail, including
                the "null pricing engine" a swap that somehow arrived without
                one reports.
        """
        ...
    def results(self) -> Results:
        """Return a frozen snapshot of the valuation, calculating first.

        Returns:
            Results: A copy of the valuation results.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def npv(self) -> float:
        """Return the swap NPV under the engine the builder attached.

        Returns:
            float: The present value.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def nominal(self) -> float:
        """Return the notional both legs accrue on.

        Returns:
            float: The single nominal, read through the swap's base.

        Raises:
            ItofinError: If the legs carry per-coupon nominals, which leaves no
                single one to report.
        """
        ...
    def fixed_rate(self) -> float:
        """Return the fixed-leg rate.

        Returns:
            float: The rate given to the builder, or the fair rate it filled in
                for a par swap.
        """
        ...

class MakeOis:
    """Market-convention builder for an OvernightIndexedSwap.

    Derives the start and end dates, both schedules and the discounting engine
    from a swap tenor and an overnight index, so the caller states conventions
    instead of hand-building two schedules. ``fixed_rate=None`` builds a par
    swap: the fair rate is computed off a temporary swap and written into the
    fixed leg, so the result prices to a zero NPV.

    The core builder is a consumed-self fluent chain, which does not cross the
    FFI boundary; this facade takes the overrides as constructor keywords and
    assembles the chain inside build(). Only five overrides are exposed; every
    other core one keeps its default, and the four the core rejects outright
    (telescopic value dates, lookback, lockout and observation shift) are
    unreachable from here by construction. The built swap already carries its
    DiscountingSwapEngine.
    """

    def __init__(
        self,
        swap_tenor: Period,
        overnight_index: OvernightIndex,
        settings: Settings,
        fixed_rate: float | None = None,
        forward_start: Period | None = None,
        effective_date: Date | None = None,
        nominal: float | None = None,
        payment_lag: int | None = None,
        discounting_term_structure: YieldTermStructure | None = None,
        averaging_method: RateAveraging | None = None,
    ) -> None:
        """Store the configuration the chain is assembled from in build().

        Args:
            swap_tenor (Period): The length of the swap.
            overnight_index (OvernightIndex): The index the overnight leg
                compounds.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
            fixed_rate (float | None): The rate of the fixed leg; None builds a
                par swap.
            forward_start (Period | None): The delay before the swap starts;
                None starts it spot, at a zero-day period.
            effective_date (Date | None): The start date; None derives it from
                the evaluation date.
            nominal (float | None): The notional; None keeps the core default.
            payment_lag (int | None): The days between accrual end and payment;
                None keeps the core default.
            discounting_term_structure (YieldTermStructure | None): The curve
                the flows discount on; None keeps the core default.
            averaging_method (RateAveraging | None): Whether the overnight
                fixings compound or are averaged; None keeps the core default.
        """
        ...
    def build(self) -> OvernightIndexedSwap:
        """Build the priced swap.

        Returns:
            OvernightIndexedSwap: The swap, already carrying its discounting
                engine.

        Raises:
            ItofinError: If effective_date is unset and no evaluation date is
                set to derive the start from; if the schedule or the overnight
                leg is degenerate; or if the par-rate fill fails to price.
        """
        ...

class EuropeanExercise:
    """A single-date exercise schedule.

    Held as the exercise trait object the swaption constructor takes, so the
    same value reaches the instrument.
    """

    def __init__(self, date: Date) -> None:
        """Build the exercise schedule.

        Args:
            date (Date): The single date the option may be exercised on.
        """
        ...

class SettlementType:
    """How a swaption settles on exercise."""

    Physical: SettlementType
    Cash: SettlementType

class SettlementMethod:
    """The settlement mechanics under a settlement type.

    Physical pairs with PhysicalOTC or PhysicalCleared, cash with
    CollateralizedCashPrice or ParYieldCurve. The consistency check runs at
    pricing time, not construction, so a mismatched pair only surfaces from
    npv().
    """

    PhysicalOTC: SettlementMethod
    PhysicalCleared: SettlementMethod
    CollateralizedCashPrice: SettlementMethod
    ParYieldCurve: SettlementMethod

class Swaption:
    """A European option to enter a vanilla swap.

    The swaption registers with the underlying swap and with the evaluation
    date on the Settings it was built with (D5). Pricing needs an engine: call
    one of the three setters before npv.
    """

    def __init__(
        self,
        swap: VanillaSwap,
        exercise: EuropeanExercise,
        settlement_type: SettlementType,
        settlement_method: SettlementMethod,
        settings: Settings,
    ) -> None:
        """Build the swaption over swap.

        Args:
            swap (VanillaSwap): The swap the option enters; it needs no
                discounting engine of its own, the swaption engine reading its
                arguments instead.
            exercise (EuropeanExercise): The single exercise date.
            settlement_type (SettlementType): Whether exercise settles
                physically or in cash.
            settlement_method (SettlementMethod): The mechanics under that
                type; an inconsistent pair surfaces from npv(), not here.
            settings (Settings): The explicit settings supplying the evaluation
                date the swaption prices against.
        """
        ...
    def set_jamshidian_engine(self, model: HullWhite) -> None:
        """Attach a Jamshidian engine so the swaption prices off Hull-White.

        The engine is European-only: a non-European exercise errors at pricing
        time.

        Args:
            model (HullWhite): The short-rate model supplying the dynamics.
        """
        ...
    def set_black_engine(self, engine: BlackSwaptionEngine) -> None:
        """Attach a Black engine, pricing off a swaption volatility surface.

        The engine is built separately, so the same one can be shared across
        swaptions. It must carry the same Settings object as this swaption: two
        different settings would price the swap and the option on different
        dates with no error raised.

        Args:
            engine (BlackSwaptionEngine): The engine and its volatility
                surface.
        """
        ...
    def set_bachelier_engine(self, engine: BachelierSwaptionEngine) -> None:
        """Attach a Bachelier engine, pricing off a normal-volatility surface.

        The same-Settings requirement as set_black_engine applies.

        Args:
            engine (BachelierSwaptionEngine): The engine and its
                normal-volatility surface.
        """
        ...
    def calculate(self) -> None:
        """Force the valuation. Idempotent.

        Raises:
            ItofinError: If no engine is attached, no evaluation date is set,
                or the (settlement type, method) pair is inconsistent, which
                the core checks here rather than at construction.
        """
        ...
    def is_calculated(self) -> bool:
        """Return whether the cached results are currently valid.

        Returns:
            bool: True when the next accessor reads the cache.
        """
        ...
    def price(self, engine: BlackSwaptionEngine) -> float:
        """Attach the Black engine and return the NPV.

        set_black_engine followed by npv, in one call. Black is the primary
        because it is the standard swaption engine; the Jamshidian and
        Bachelier engines keep their own setters.

        Args:
            engine (BlackSwaptionEngine): The engine to install and price on.

        Returns:
            float: The swaption value.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def results(self) -> Results:
        """Return a frozen snapshot of the valuation, calculating first.

        Returns:
            Results: A copy of the valuation results.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def npv(self) -> float:
        """Return the swaption NPV under the attached engine.

        Returns:
            float: The present value.

        Raises:
            ItofinError: If no engine is attached or the (settlement type,
                method) pair is inconsistent.
        """
        ...

class CapFloorType:
    """Whether the instrument caps, floors or collars its floating leg.

    Collar reaches an instrument only through a raw coupon-vector constructor:
    CapFloor.collar here, or the YoYInflationCapFloor ones on the inflation
    side. MakeCapFloor refuses it, so CapFloor(...) does not accept it."""

    Cap: CapFloorType
    Floor: CapFloorType
    Collar: CapFloorType

class CapFloor:
    """A cap, floor or collar over a floating (ibor) leg.

    The constructor runs the standard market builder MakeCapFloor: its leg
    carries a unit nominal and one strike, and a zero forward_start excludes the
    spot caplet, so the leg is one coupon shorter than the schedule - that is
    what lets the cap price without a historical index fixing at the evaluation
    date.

    The cap/floor/collar staticmethods take an IborLeg the caller laid out
    instead and cap exactly it, spot caplet and all. They are the only route to
    a collar on this side, and the route a hand-built leg's own notional, day
    counter and fixing days reach the coupons by. Either way the core pads a
    short strike list across every coupon by repeating its last entry."""

    def __init__(
        self,
        cap_floor_type: CapFloorType,
        tenor: Period,
        ibor_index: IborIndex,
        strike: float,
        forward_start: Period,
        settings: Settings,
    ) -> None:
        """Build a standard market cap or floor through MakeCapFloor.

        Args:
            cap_floor_type (CapFloorType): Cap or Floor; the builder refuses
                Collar.
            tenor (Period): The length of the capped leg.
            ibor_index (IborIndex): The index the floating leg fixes off.
            strike (float): The single strike, padded across every coupon.
            forward_start (Period): The delay before the leg starts; a zero
                period excludes the spot caplet.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Raises:
            ItofinError: If cap_floor_type is Collar, if the derived schedule
                is degenerate, or if the start has to be derived and no
                evaluation date is set.
        """
        ...
    @staticmethod
    def cap(leg: IborLeg, cap_rates: list[float], settings: Settings) -> CapFloor:
        """Build a cap over the coupons leg builds, struck at cap_rates.

        Unlike the constructor this keeps whatever leg it is given: the spot
        caplet stays, and the leg's own notional, day counter and fixing days
        reach the coupons.

        Args:
            leg (IborLeg): The leg whose coupons are capped.
            cap_rates (list[float]): The cap strikes, padded to the leg length
                by repeating the last entry.
            settings (Settings): The explicit settings the instrument resolves
                its dates against.

        Returns:
            CapFloor: The cap over that leg.

        Raises:
            ItofinError: On an empty cap_rates list, or on whatever building
                the leg's coupons reports, a missing notional above all.
        """
        ...
    @staticmethod
    def floor(
        leg: IborLeg, floor_rates: list[float], settings: Settings
    ) -> CapFloor:
        """Build a floor over the coupons leg builds, struck at floor_rates.

        Args:
            leg (IborLeg): The leg whose coupons are floored.
            floor_rates (list[float]): The floor strikes, padded as cap() pads.
            settings (Settings): The explicit settings the instrument resolves
                its dates against.

        Returns:
            CapFloor: The floor over that leg.

        Raises:
            ItofinError: Fallible as cap(), on an empty list or a leg whose
                coupons cannot be built.
        """
        ...
    @staticmethod
    def collar(
        leg: IborLeg,
        cap_rates: list[float],
        floor_rates: list[float],
        settings: Settings,
    ) -> CapFloor:
        """Build a collar: long the cap at cap_rates, short the floor at floor_rates.

        The collar is worth the one less the other, and this is the only route
        to one over a floating leg.

        Args:
            leg (IborLeg): The leg whose coupons are collared.
            cap_rates (list[float]): The cap strikes, padded as cap() pads.
            floor_rates (list[float]): The floor strikes, padded the same way.
            settings (Settings): The explicit settings the instrument resolves
                its dates against.

        Returns:
            CapFloor: The collar over that leg.

        Raises:
            ItofinError: On either list being empty, both being required, or on
                a leg whose coupons cannot be built.
        """
        ...
    def cap_rates(self) -> list[float]:
        """Return the cap strikes, one per coupon.

        Returns:
            list[float]: The cap strikes; empty for a floor.
        """
        ...
    def floor_rates(self) -> list[float]:
        """Return the floor strikes, one per coupon.

        Returns:
            list[float]: The floor strikes; empty for a cap.
        """
        ...
    def coupon_count(self) -> int:
        """Return the number of optionlets.

        Returns:
            int: One per floating coupon on the leg.
        """
        ...
    def set_black_engine(self, engine: BlackCapFloorEngine) -> None:
        """Attach a Black engine, pricing each optionlet off a volatility surface.

        The engine is built separately, so the same one can be shared across
        instruments. It must resolve its dates against the same Settings object
        as this cap/floor: two different settings would price the leg and the
        optionlets on different dates with no error raised.

        Args:
            engine (BlackCapFloorEngine): The engine and its optionlet
                volatility surface.
        """
        ...
    def calculate(self) -> None:
        """Force the valuation. Idempotent.

        Raises:
            ItofinError: If no engine is attached, no evaluation date is set,
                or the engine refuses the instrument.
        """
        ...
    def is_calculated(self) -> bool:
        """Return whether the cached results are currently valid.

        The Black engine observes its volatility handle, so moving a quote the
        engine was built over reaches the cap and flips this back to False.

        Returns:
            bool: True when the next accessor reads the cache.
        """
        ...
    def price(self, engine: BlackCapFloorEngine) -> float:
        """Attach engine and return the NPV.

        Args:
            engine (BlackCapFloorEngine): The engine to install and price on.

        Returns:
            float: The cap/floor value.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def results(self) -> Results:
        """Return a frozen snapshot of the valuation, calculating first.

        Returns:
            Results: A copy of the valuation results.

        Raises:
            ItofinError: On anything that makes the valuation fail.
        """
        ...
    def npv(self) -> float:
        """Return the cap/floor NPV under the attached engine.

        Returns:
            float: The present value.

        Raises:
            ItofinError: If no engine is attached, which the core reports as
                "null pricing engine".
        """
        ...

class ProtectionSide:
    """Which leg of a default-protection contract a party holds: the buyer pays
    the premium leg and receives the default payment, the seller the reverse."""

    Buyer: ProtectionSide
    Seller: ProtectionSide

class PricingModel:
    """The model a quoted contract is inverted under by
    CreditDefaultSwap.implied_hazard_rate: Midpoint is not ISDA conform, Isda
    carries the three fidelity flags the core fixes at that call site."""

    Midpoint: PricingModel
    Isda: PricingModel

class CreditDefaultSwap:
    """A credit-default swap quoted as a running spread.

    __init__ takes the C++ default terms with settles_accrual and
    pays_at_default_time quoted; with_terms additionally exposes
    protection_start and rebates_accrual.

    These direct constructors keep the remaining CdsTerms fields at their core
    defaults: claim (a face-value claim, which needs a claim facade that does
    not exist yet), last_period_day_counter, upfront_date and
    cash_settlement_days. trade_date and an upfront are not set here but are
    reachable through MakeCreditDefaultSwap (with_trade_date and the upfront_rate
    constructor argument)."""

    def __init__(
        self,
        side: ProtectionSide,
        notional: float,
        spread: float,
        schedule: Schedule,
        payment_convention: BusinessDayConvention,
        day_counter: DayCounter,
        settles_accrual: bool,
        pays_at_default_time: bool,
        settings: Settings,
    ) -> None: ...
    @staticmethod
    def with_terms(
        side: ProtectionSide,
        notional: float,
        spread: float,
        schedule: Schedule,
        payment_convention: BusinessDayConvention,
        day_counter: DayCounter,
        settings: Settings,
        protection_start: Date | None = None,
        settles_accrual: bool = True,
        pays_at_default_time: bool = True,
        rebates_accrual: bool = True,
    ) -> CreditDefaultSwap:
        """A contract quoting the terms __init__ defaults. protection_start is
        the first date a default triggers the contract; None takes the
        schedule's first date, which is what __init__ does. settings precedes
        the defaulted terms because a required argument cannot follow an
        optional one."""
        ...
    def set_engine(self, engine: MidPointCdsEngine) -> None:
        """Price the contract off a default-probability and a discount curve.
        The engine must resolve its dates against the same Settings object as
        this contract."""
        ...
    def set_isda_engine(self, engine: IsdaCdsEngine) -> None:
        """Price the contract under the ISDA standard model. A separate setter
        because the two engine classes are unrelated; the same same-Settings
        rule applies, and the ISDA engine additionally refuses curves outside
        its specification when the contract prices."""
        ...
    def calculate(self) -> None:
        """Forces the valuation. Idempotent."""
        ...
    def is_calculated(self) -> bool: ...
    def price(self, engine: MidPointCdsEngine) -> float:
        """set_engine followed by npv, in one call. The mid-point engine is the
        primary; set_isda_engine stays a separate setter."""
        ...
    def results(self) -> Results: ...
    def npv(self) -> float:
        """Raises ItofinError with no engine attached."""
        ...
    def fair_spread(self) -> float:
        """The running spread that prices the contract at zero. Raises
        ItofinError with no engine attached, and when the engine priced a
        worthless premium leg and so provided no fair spread."""
        ...
    def fair_upfront(self) -> float:
        """The upfront that prices the contract at zero, as a fraction of the
        notional. Raises ItofinError as fair_spread does."""
        ...
    def notional(self) -> float:
        """The notional the premium and the protection are quoted on."""
        ...
    def accrual_rebate_amount(self) -> float | None:
        """The accrued coupon the protection seller rebates, or None when the
        contract does not rebate accrual at all. A contract traded in the past
        still carries the flow, with a real amount but a past settlement date
        that keeps it out of the value, so None means the flag rather than a
        stale trade."""
        ...
    def accrual_rebate_date(self) -> Date | None:
        """The date the accrual rebate settles on, the same cash-settlement date
        the upfront pays on. None on the same terms as accrual_rebate_amount."""
        ...
    def coupon_leg_npv(self) -> float: ...
    def default_leg_npv(self) -> float: ...
    def implied_hazard_rate(
        self,
        target_npv: float,
        discount: YieldTermStructure,
        day_counter: DayCounter,
        recovery_rate: float,
        accuracy: float,
        model: PricingModel,
    ) -> float:
        """The flat hazard rate at which this contract is worth target_npv.

        The solve stands on its own engine rather than on whichever one
        set_engine attached, so there is no probability-curve argument:
        day_counter counts the flat curve the solve builds, not the contract.
        Under PricingModel.Isda both it and discount must count Act/365 (Fixed),
        which is what the ISDA engine requires of its curves.

        Raises ItofinError on a malformed contract and when the solve does not
        converge, which includes a pricing failure at some hazard rate."""
        ...

class MakeCreditDefaultSwap:
    """Market-convention builder for a CreditDefaultSwap: derives the premium
    schedule from a maturity and the post-Big-Bang CDS conventions, and takes
    the trade date from the evaluation date settings carries.

    An unset optional keeps the core default: a Buyer side, a nominal of 1, no
    upfront, a 3M coupon tenor, the pre-CDS2015 DateGeneration.CDS rule, a
    Following roll, an Act/360 day counter and three cash-settlement days. Only
    the term-date quotation is exposed; the tenor and explicit-schedule ones and
    the accrual-rebate flag are not, the latter being reachable through
    CreditDefaultSwap.with_terms."""

    def __init__(
        self,
        term_date: Date,
        running_spread: float,
        settings: Settings,
        nominal: float | None = None,
        upfront_rate: float | None = None,
        side: ProtectionSide | None = None,
        trade_date: Date | None = None,
    ) -> None:
        """trade_date overrides the evaluation date the trade is otherwise dated
        off, which is how a contract traded in the past is built."""
        ...
    def build(self) -> CreditDefaultSwap:
        """The built contract, which carries no engine. Raises ItofinError with
        no evaluation date set, the trade date being derived from it."""
        ...

class ZeroCouponInflationSwap:
    """One fixed flow against one inflation-indexed flow, both exchanged at
    maturity.

    fixed_rate is the K that at inception matches the inflation growth.
    SwapType names the inflation leg, so a Payer pays inflation and receives
    fixed.

    maturity is pre-adjustment: each leg's payment date is it rolled on that
    leg's calendar and convention, while the year fraction behind the fixed
    amount stays on the raw date. inflation_calendar and inflation_convention
    fall back to the fixed-leg ones when None.

    The core omits adjust_inf_obs_dates from its own signature, so there is
    nothing to expose here; the leg and cash-flow accessors are not surfaced
    either, there being no cash-flow facade."""

    def __init__(
        self,
        swap_type: SwapType,
        nominal: float,
        start_date: Date,
        maturity: Date,
        fixed_calendar: Calendar,
        fixed_convention: BusinessDayConvention,
        day_counter: DayCounter,
        fixed_rate: float,
        inflation_index: ZeroInflationIndex,
        observation_lag: Period,
        observation_interpolation: CpiInterpolationType,
        inflation_calendar: Calendar | None,
        inflation_convention: BusinessDayConvention | None,
        settings: Settings,
    ) -> None:
        """Raises ItofinError when the observation lag is too short for the
        index to observe fixings that exist."""
        ...
    def set_engine(self, engine: DiscountingSwapEngine) -> None:
        """Price the swap off a discount curve. The engine must resolve its
        dates against the same Settings object as this swap."""
        ...
    def calculate(self) -> None:
        """Forces the valuation. Idempotent."""
        ...
    def is_calculated(self) -> bool: ...
    def price(self, engine: DiscountingSwapEngine) -> float:
        """set_engine followed by npv, in one call."""
        ...
    def results(self) -> Results: ...
    def npv(self) -> float:
        """Raises ItofinError with no engine attached, and with no curve linked
        into the index."""
        ...
    def fair_rate(self) -> float:
        """The index ratio de-compounded over the swap's own year fraction.

        Needs no engine - it reads the indexed flow rather than any priced
        result - but does need the index linked to a curve."""
        ...
    def fixed_leg_npv(self) -> float: ...
    def inflation_leg_npv(self) -> float: ...
    def fixed_leg_bps(self) -> float:
        """The fixed leg's sensitivity to a basis point on the quoted rate,
        computed in closed form rather than read off the engine, whose own leg
        BPS is zero for a non-coupon flow."""
        ...
    def maturity_date(self) -> Date:
        """The contract maturity, raw and pre-adjustment - not either leg's
        payment date."""
        ...
    def obs_date(self) -> Date:
        """The date the maturity fixing is observed at, maturity less the
        observation lag, unsnapped."""
        ...
    def inflation_fixing_date(self) -> Date:
        """The same date as obs_date, read off the indexed flow rather than off
        the swap. Both names are kept because both exist in the core."""
        ...

class YearOnYearInflationSwap:
    """A fixed leg against a leg of year-on-year inflation coupons, both paid
    over a schedule.

    SwapType names the fixed leg, so a Payer pays fixed and receives inflation -
    the opposite reading from ZeroCouponInflationSwap, where it names the
    inflation leg.

    The two schedules are independent inputs. The fixed leg takes its payment
    calendar from its own schedule while the year-on-year leg pays on
    payment_calendar; both adjust with payment_convention. spread is added to
    every forecast rate on the year-on-year leg.

    Pricing needs an engine: call set_engine first. Every priced accessor drives
    the calculation, so all of them mutate."""

    def __init__(
        self,
        swap_type: SwapType,
        nominal: float,
        fixed_schedule: Schedule,
        fixed_rate: float,
        fixed_day_count: DayCounter,
        yoy_schedule: Schedule,
        yoy_index: YoYInflationIndex,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
        spread: float,
        yoy_day_count: DayCounter,
        payment_calendar: Calendar,
        payment_convention: BusinessDayConvention,
        settings: Settings,
    ) -> None:
        """Raises ItofinError when either leg cannot be built, notably from an
        observation lag that leaves a coupon unbuildable."""
        ...
    def set_engine(self, engine: DiscountingSwapEngine) -> None:
        """The engine must resolve its dates against the same Settings object
        this swap was built with."""
        ...
    def calculate(self) -> None:
        """Forces the valuation. Idempotent."""
        ...
    def is_calculated(self) -> bool: ...
    def price(self, engine: DiscountingSwapEngine) -> float:
        """set_engine followed by npv, in one call."""
        ...
    def results(self) -> Results: ...
    def npv(self) -> float: ...
    def fair_rate(self) -> float:
        """The fixed rate that would price the swap at zero, recovered from the
        NPV and the fixed leg's BPS."""
        ...
    def fair_spread(self) -> float:
        """The spread over the index that would price the swap at zero,
        recovered off the year-on-year leg."""
        ...
    def fixed_leg_npv(self) -> float: ...
    def yoy_leg_npv(self) -> float: ...
    def fixed_rate(self) -> float: ...
    def spread(self) -> float: ...

class MakeYoYInflationCapFloor:
    """The standard market builder for a year-on-year inflation cap or floor.

    It derives an annual year-on-year leg from a length in years, trims that leg
    to the optionlets asked for, and strikes it either at an explicit strike or
    at the money off atm_strike. Exactly one of the two is required: the core
    refuses both together and neither at all, at build time rather than at the
    setters, so both surface from build().

    The core builder is a consumed-self fluent chain, which does not cross the
    FFI boundary; this facade takes the whole configuration up front and
    assembles the chain inside build(), as MakeVanillaSwap does. An unset
    optional leaves the core default in place: a 1,000,000 nominal, a
    ModifiedFollowing payment roll, a 30/360 bond-basis day counter, no fixing
    days, every optionlet kept and no forward start.

    Trimming happens before the at-the-money fill, so as_optionlet and
    first_caplet_excluded change what an unset strike resolves to: the rate that
    reprices whatever survives, not the whole leg's.

    CapFloorType.Collar has no path here - the builder carries a single strike,
    and a collar needs two strike vectors - so a collar is built through
    YoYInflationCapFloor.collar over a leg of its own instead."""

    def __init__(
        self,
        cap_floor_type: CapFloorType,
        index: YoYInflationIndex,
        length: int,
        calendar: Calendar,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
        settings: Settings,
        nominal: float | None = None,
        effective_date: Date | None = None,
        payment_day_counter: DayCounter | None = None,
        payment_adjustment: BusinessDayConvention | None = None,
        fixing_days: int | None = None,
        engine: YoYInflationCapFloorEngine | None = None,
        as_optionlet: bool = False,
        forward_start: Period | None = None,
        first_caplet_excluded: bool = False,
        strike: float | None = None,
        atm_strike: YieldTermStructure | None = None,
    ) -> None: ...
    def build(self) -> YoYInflationCapFloor:
        """Raises ItofinError when both strike and atm_strike are given and when
        neither is; when the start date has to be derived and no evaluation date
        is set; and on whatever the leg construction and the at-the-money fill
        report."""
        ...

class YoYInflationCapFloor:
    """A cap, floor or collar over a year-on-year inflation leg.

    Built either through MakeYoYInflationCapFloor, the standard market builder,
    or through the raw constructors below, which take the coupon vector
    YoYInflationLeg.coupons() hands back (#848). The raw route is the only one
    that reaches a collar: the builder carries a single strike.

    Unlike a nominal cap/floor this instrument keeps its first optionlet, so the
    strip spans its leg exactly and cap - floor is the year-on-year swap.

    Pricing needs an engine: call set_engine before npv."""

    @staticmethod
    def new(
        cap_floor_type: CapFloorType,
        coupons: list[YoYInflationCoupon],
        cap_rates: list[float],
        floor_rates: list[float],
        settings: Settings,
    ) -> YoYInflationCapFloor:
        """Each strike vector is padded to the leg length by repeating its last
        entry. Raises ItofinError on an empty leg, and on a strike vector the
        type needs and did not get: a cap or a collar needs cap rates, a floor
        or a collar floor rates."""
        ...
    @staticmethod
    def cap(
        coupons: list[YoYInflationCoupon],
        strikes: list[float],
        settings: Settings,
    ) -> YoYInflationCapFloor: ...
    @staticmethod
    def floor(
        coupons: list[YoYInflationCoupon],
        strikes: list[float],
        settings: Settings,
    ) -> YoYInflationCapFloor: ...
    @staticmethod
    def collar(
        coupons: list[YoYInflationCoupon],
        cap_rates: list[float],
        floor_rates: list[float],
        settings: Settings,
    ) -> YoYInflationCapFloor:
        """Long the cap at cap_rates, short the floor at floor_rates."""
        ...
    @staticmethod
    def with_strikes(
        cap_floor_type: CapFloorType,
        coupons: list[YoYInflationCoupon],
        strikes: list[float],
        settings: Settings,
    ) -> YoYInflationCapFloor:
        """strikes are cap rates for a Cap and floor rates for a Floor. Raises
        ItofinError on an empty strikes and on a Collar, which needs two vectors
        and has collar() for a constructor."""
        ...
    def cap_rates(self) -> list[float]: ...
    def floor_rates(self) -> list[float]: ...
    def coupon_count(self) -> int: ...
    def start_date(self) -> Date: ...
    def maturity_date(self) -> Date: ...
    def atm_rate(self, discount_curve: YieldTermStructure) -> float:
        """The strike at which the leg reprices on discount_curve. Raises
        ItofinError on an unlinked curve, a curve with no reference date and a
        leg with no basis-point sensitivity to solve over."""
        ...
    def set_engine(self, engine: YoYInflationCapFloorEngine) -> None:
        """The engine must resolve its dates against the same Settings object
        this cap/floor was built with."""
        ...
    def calculate(self) -> None:
        """Forces the valuation. Idempotent."""
        ...
    def is_calculated(self) -> bool: ...
    def price(self, engine: YoYInflationCapFloorEngine) -> float:
        """set_engine followed by npv, in one call, replacing whatever engine
        the factory installed."""
        ...
    def results(self) -> Results: ...
    def npv(self) -> float: ...
