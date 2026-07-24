# Hand-written stubs for itofin.models; sync manually with src/heston.rs,
# src/hullwhite.rs and src/calibration.rs (#517).

from itofin import Settings
from itofin.indexes import Euribor
from itofin.instruments import OptionType
from itofin.optimization import EndCriteria, LevenbergMarquardt
from itofin.processes import HestonProcess
from itofin.termstructures import YieldTermStructure
from itofin.time import Calendar, Date, DayCounter, Period

class HestonModel:
    """The five-parameter calibrated Heston model."""

    def __init__(self, process: HestonProcess) -> None: ...
    def theta(self) -> float: ...
    def kappa(self) -> float: ...
    def sigma(self) -> float: ...
    def rho(self) -> float: ...
    def v0(self) -> float: ...
    def calibrate(
        self,
        helpers: list[HestonModelHelper],
        method: LevenbergMarquardt,
        end_criteria: EndCriteria,
        integration_order: int,
    ) -> None: ...

class HullWhite:
    """The one-factor Hull-White short-rate model."""

    def __init__(self, curve: YieldTermStructure, a: float, sigma: float) -> None: ...
    def a(self) -> float: ...
    def sigma(self) -> float: ...
    def r0(self) -> float: ...
    def discount_bond_option(
        self, option_type: OptionType, strike: float, maturity: float, bond_maturity: float
    ) -> float: ...
    def calibrate(
        self,
        helpers: list[SwaptionHelper],
        method: LevenbergMarquardt,
        end_criteria: EndCriteria,
        fix_reversion: bool,
    ) -> None: ...

class HestonModelHelper:
    """A Black-vol calibration helper over a flat-vol surface."""

    def __init__(
        self,
        maturity: Period,
        calendar: Calendar,
        s0: float,
        strike: float,
        volatility: float,
        risk_free_rate: float,
        dividend_yield: float,
        error_type: CalibrationErrorType,
        reference_date: Date,
        day_counter: DayCounter,
        settings: Settings,
    ) -> None: ...
    def calibration_error(self) -> float: ...

class SwaptionHelper:
    """A co-terminal swaption calibration instrument."""

    def __init__(
        self,
        maturity: Period,
        length: Period,
        volatility: float,
        index: Euribor,
        fixed_leg_tenor: Period,
        fixed_leg_day_counter: DayCounter,
        floating_leg_day_counter: DayCounter,
        curve: YieldTermStructure,
        error_type: CalibrationErrorType,
        nominal: float,
    ) -> None: ...
    def calibration_error(self) -> float: ...

class CalibrationErrorType:
    """How market and model prices are compared during calibration."""

    RelativePriceError: CalibrationErrorType
    PriceError: CalibrationErrorType
    ImpliedVolError: CalibrationErrorType
