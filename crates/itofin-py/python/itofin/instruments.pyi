# Hand-written stubs for itofin.instruments; sync manually with src/option.rs,
# src/swap.rs, src/swaption.rs, src/capfloor.rs and src/credit.rs (#517).

from itofin import Settings
from itofin.indexes import Euribor
from itofin.models import HestonModel, HullWhite
from itofin.pricingengines import (
    BlackCapFloorEngine,
    BlackSwaptionEngine,
    MidPointCdsEngine,
)
from itofin.processes import BlackScholesProcess
from itofin.termstructures import YieldTermStructure
from itofin.time import (
    BusinessDayConvention,
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
    """A single-asset European option."""

    def __init__(
        self, option_type: OptionType, strike: float, expiry: Date, settings: Settings
    ) -> None: ...
    def set_engine(self, process: BlackScholesProcess) -> None: ...
    def set_heston_engine(self, model: HestonModel, integration_order: int) -> None: ...
    def npv(self) -> float: ...
    def delta(self) -> float: ...
    def gamma(self) -> float: ...
    def theta(self) -> float: ...
    def vega(self) -> float: ...
    def rho(self) -> float: ...
    def dividend_rho(self) -> float: ...

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

class CreditDefaultSwap:
    """A credit-default swap quoted as a running spread.

    __init__ takes the C++ default terms with settles_accrual and
    pays_at_default_time quoted; with_terms additionally exposes
    protection_start and rebates_accrual.

    Four of the eight CdsTerms fields are not exposed and keep their core
    defaults: claim (a face-value claim, which needs a claim facade that does
    not exist yet), last_period_day_counter, trade_date and
    cash_settlement_days. The upfront-quoted constructor is not ported in the
    core either, so the contract never carries an upfront."""

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
    def npv(self) -> float:
        """Raises ItofinError with no engine attached."""
        ...
    def fair_spread(self) -> float:
        """The running spread that prices the contract at zero. Raises
        ItofinError with no engine attached, and when the engine priced a
        worthless premium leg and so provided no fair spread."""
        ...
    def coupon_leg_npv(self) -> float: ...
    def default_leg_npv(self) -> float: ...
