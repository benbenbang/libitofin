# Hand-written stubs for itofin.indexes; sync manually with src/hullwhite.rs, src/helpers.rs
# and src/swapindex.rs (#517).

from itofin import Settings
from itofin.termstructures import YieldTermStructure
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Period,
)

class Euribor:
    """The Euribor IBOR index family."""

    def __init__(
        self, tenor: Period, curve: YieldTermStructure | None, settings: Settings
    ) -> None: ...
    @staticmethod
    def three_months(curve: YieldTermStructure, settings: Settings) -> Euribor: ...
    @staticmethod
    def six_months(curve: YieldTermStructure, settings: Settings) -> Euribor: ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool) -> float: ...

class Estr:
    """The Euro Short-Term Rate overnight index. Pass curve=None to build it over
    an empty forwarding handle (the form the OIS bootstrap needs)."""

    def __init__(self, curve: YieldTermStructure | None, settings: Settings) -> None: ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool) -> float: ...

class SwapIndex:
    """The index whose fixing is the fair rate of an on-the-fly vanilla swap,
    assembled from the index tenor, the forecasting Euribor index and the
    fixed-leg conventions.

    The currency is hard-coded to EUR: there is no Currency facade yet, and the
    currency is inert for every ported consumer."""

    def __init__(
        self,
        family_name: str,
        tenor: Period,
        settlement_days: int,
        calendar: Calendar,
        fixed_leg_tenor: Period,
        fixed_leg_convention: BusinessDayConvention,
        fixed_leg_day_counter: DayCounter,
        ibor_index: Euribor,
        settings: Settings,
    ) -> None:
        """Forecasts and discounts off the ibor index's forwarding curve."""
        ...
    @staticmethod
    def with_exogenous_discount(
        family_name: str,
        tenor: Period,
        settlement_days: int,
        calendar: Calendar,
        fixed_leg_tenor: Period,
        fixed_leg_convention: BusinessDayConvention,
        fixed_leg_day_counter: DayCounter,
        ibor_index: Euribor,
        discount: YieldTermStructure,
        settings: Settings,
    ) -> SwapIndex:
        """Forecasts off the ibor index's forwarding curve but discounts off the
        separate discount curve."""
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool = False) -> float:
        """The underlying swap's fair rate, the at-the-money forward the
        volatility cubes read."""
        ...
    def fixed_leg_tenor(self) -> Period: ...
    def exogenous_discount(self) -> bool: ...
