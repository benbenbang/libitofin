# Hand-written stubs for itofin.instruments; sync manually with src/option.rs,
# src/swap.rs and src/swaption.rs (#517).

from itofin import Settings
from itofin.indexes import Euribor
from itofin.models import HestonModel, HullWhite
from itofin.processes import BlackScholesProcess
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter, Schedule

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
    def set_engine(self, curve: FlatForward, settings: Settings) -> None: ...
    def fair_rate(self) -> float: ...
    def npv(self) -> float: ...
    def nominal(self) -> float: ...
    def fixed_rate(self) -> float: ...

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
    def npv(self) -> float: ...
