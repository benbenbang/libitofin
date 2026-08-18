# Hand-written stubs for itofin.instruments; sync manually with src/option.rs,
# src/swap.rs, src/swaption.rs, src/capfloor.rs, src/credit.rs and
# src/inflation.rs (#517).

from itofin import Settings
from itofin.indexes import (
    CpiInterpolationType,
    Euribor,
    YoYInflationIndex,
    ZeroInflationIndex,
)
from itofin.models import HestonModel, HullWhite
from itofin.pricingengines import (
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
from itofin.termstructures import YieldTermStructure
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Period,
    Schedule,
)

class OptionType:
    """The call/put flag."""

    Call: OptionType
    Put: OptionType

class VanillaOption:
    """A single-asset vanilla option: European by construction, American
    through the american classmethod."""

    def __init__(
        self, option_type: OptionType, strike: float, expiry: Date, settings: Settings
    ) -> None: ...
    @classmethod
    def american(
        cls,
        option_type: OptionType,
        strike: float,
        earliest: Date,
        latest: Date,
        settings: Settings,
    ) -> VanillaOption:
        """The option exercisable at any time over [earliest, latest], paying on
        exercise. Raises ItofinError when earliest is after latest."""
        ...
    def set_engine(self, process: BlackScholesProcess) -> None: ...
    def set_heston_engine(self, model: HestonModel, integration_order: int) -> None: ...
    def set_mc_engine(self, engine: MCEuropeanEngine) -> None: ...
    def set_mc_heston_engine(self, engine: MCEuropeanHestonEngine) -> None: ...
    def set_mc_american_engine(self, engine: MCAmericanEngine) -> None:
        """Attaches the Monte Carlo American engine. A European-exercise option
        raises ItofinError ("wrong exercise given") when priced on it."""
        ...
    def npv(self) -> float: ...
    def delta(self) -> float: ...
    def gamma(self) -> float: ...
    def theta(self) -> float: ...
    def vega(self) -> float: ...
    def rho(self) -> float: ...
    def dividend_rho(self) -> float: ...
    def error_estimate(self) -> float:
        """The standard error on the present value. Raises ItofinError on the
        engines that do not produce one, which is every analytic engine here."""
        ...
    def exercise_probability(self) -> float:
        """The fraction of simulated paths exercised before expiry. Raises
        ItofinError on every engine that does not report it - only
        MCAmericanEngine does."""
        ...

class SwapType:
    """Which side of the named leg the swap is seen from."""

    Payer: SwapType
    Receiver: SwapType

class VanillaSwap:
    """A fixed-vs-Ibor interest-rate swap."""

    def __init__(
        self,
        swap_type: SwapType,
        nominal: float,
        fixed_schedule: Schedule,
        fixed_rate: float,
        fixed_day_count: DayCounter,
        float_schedule: Schedule,
        ibor_index: Euribor,
        spread: float,
        floating_day_count: DayCounter,
        settings: Settings,
    ) -> None: ...
    def set_engine(self, curve: YieldTermStructure, settings: Settings) -> None: ...
    def fair_rate(self) -> float: ...
    def npv(self) -> float: ...
    def nominal(self) -> float: ...
    def fixed_rate(self) -> float: ...

class MakeVanillaSwap:
    """Market-convention builder for a VanillaSwap: derives both schedules, the
    fixed-leg tenor and day count and the discounting engine from a swap tenor
    and an Ibor index. ``fixed_rate=None`` builds a par swap. The built swap
    already carries its DiscountingSwapEngine."""

    def __init__(
        self,
        swap_tenor: Period,
        ibor_index: Euribor,
        settings: Settings,
        fixed_rate: float | None = None,
        forward_start: Period | None = None,
        effective_date: Date | None = None,
        nominal: float | None = None,
        fixed_leg_tenor: Period | None = None,
        fixed_leg_day_count: DayCounter | None = None,
    ) -> None: ...
    def build(self) -> VanillaSwap: ...

class EuropeanExercise:
    """A single-date exercise schedule."""

    def __init__(self, date: Date) -> None: ...

class SettlementType:
    """How a swaption settles on exercise."""

    Physical: SettlementType
    Cash: SettlementType

class SettlementMethod:
    """The settlement mechanics under a settlement type."""

    PhysicalOTC: SettlementMethod
    PhysicalCleared: SettlementMethod
    CollateralizedCashPrice: SettlementMethod
    ParYieldCurve: SettlementMethod

class Swaption:
    """A European option to enter a vanilla swap."""

    def __init__(
        self,
        swap: VanillaSwap,
        exercise: EuropeanExercise,
        settlement_type: SettlementType,
        settlement_method: SettlementMethod,
        settings: Settings,
    ) -> None: ...
    def set_jamshidian_engine(self, model: HullWhite) -> None: ...
    def set_black_engine(self, engine: BlackSwaptionEngine) -> None:
        """Price off a swaption volatility surface instead of a short-rate
        model. The engine must carry the same Settings object as this swaption."""
        ...
    def npv(self) -> float: ...

class CapFloorType:
    """Whether the instrument caps or floors its floating leg.

    The core enum's third variant, Collar, is not exposed: MakeCapFloor rejects
    it and the raw-leg constructor needs an IborLeg facade that does not exist
    yet, so a collar has no construction path from Python."""

    Cap: CapFloorType
    Floor: CapFloorType

class CapFloor:
    """A cap or floor over a floating (ibor) leg, built through the standard
    market builder MakeCapFloor.

    The leg carries a unit nominal and one strike, padded across every coupon. A
    zero forward_start excludes the spot caplet, so the leg is one coupon shorter
    than the schedule: that is what lets the cap price without a historical index
    fixing at the evaluation date."""

    def __init__(
        self,
        cap_floor_type: CapFloorType,
        tenor: Period,
        ibor_index: Euribor,
        strike: float,
        forward_start: Period,
        settings: Settings,
    ) -> None: ...
    def cap_rates(self) -> list[float]: ...
    def floor_rates(self) -> list[float]: ...
    def coupon_count(self) -> int: ...
    def set_black_engine(self, engine: BlackCapFloorEngine) -> None:
        """Price each optionlet off an optionlet volatility surface. The engine
        must resolve its dates against the same Settings object as this
        cap/floor."""
        ...
    def npv(self) -> float:
        """Raises ItofinError with no engine attached."""
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
    and a collar needs two strike vectors over a coupon leg Python cannot build
    (#848)."""

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
    """A cap or floor over a year-on-year inflation leg.

    Built only through MakeYoYInflationCapFloor: the core's raw constructors take
    a vector of concrete year-on-year coupons, which Python has no facade to
    build (#848), so there is no direct constructor here.

    Unlike a nominal cap/floor this instrument keeps its first optionlet, so the
    strip spans its leg exactly and cap - floor is the year-on-year swap.

    Pricing needs an engine: call set_engine before npv."""

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
    def npv(self) -> float: ...
