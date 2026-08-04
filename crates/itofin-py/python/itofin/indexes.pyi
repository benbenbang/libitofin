# Hand-written stubs for itofin.indexes; sync manually with src/hullwhite.rs, src/helpers.rs,
# src/swapindex.rs and src/inflation.rs (#517).

from itofin import Settings
from itofin.termstructures import YieldTermStructure, ZeroInflationTermStructure
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

class CpiInterpolationType:
    """How a CPI observation interpolates between the index fixings bracketing
    it. Flat reads the fixing of the lagged period outright; Linear advances
    from it to the next period's fixing by how far the observation date has run
    into its own period."""

    Flat: CpiInterpolationType
    Linear: CpiInterpolationType

class ZeroInflationIndex:
    """A price index publishing one level per period, reading back either a
    stored figure or a forecast off its inflation curve.

    The curve is reached through a relinkable handle the index owns, so an
    index can be built before the curve it forecasts off exists. The handle
    starts empty and a forecast before any link raises ItofinError; link_to
    fills it."""

    @staticmethod
    def uk_rpi(settings: Settings) -> ZeroInflationIndex:
        """The UK Retail Price Index: monthly, one-month availability lag."""
        ...
    @staticmethod
    def uk_hicp(settings: Settings) -> ZeroInflationIndex:
        """The UK harmonised index of consumer prices."""
        ...
    @staticmethod
    def eu_hicp(settings: Settings) -> ZeroInflationIndex:
        """The euro-area harmonised index of consumer prices."""
        ...
    def name(self) -> str: ...
    def add_fixing(self, fixing_date: Date, value: float) -> None:
        """Records a published figure across the whole inflation period it
        describes, so a later read on any day inside that period finds it."""
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool = False) -> float:
        """The fixing at fixing_date, stored or forecast off the linked curve.

        forecast_todays_fixing is accepted and ignored, as in the core:
        needs_forecast alone decides between history and forecast. A date the
        store should cover but does not is an error, and a forecast with no
        curve linked raises the empty-handle error."""
        ...
    def last_fixing_date(self) -> Date:
        """The first day of the inflation period the latest stored figure
        describes. Raises ItofinError on an index with no history."""
        ...
    def link_to(self, curve: ZeroInflationTermStructure) -> None:
        """Points the index at curve, so every forecast from here on compounds
        off it.

        Takes the ZeroInflationTermStructure base, so any subclass links. It is
        the curve behind that facade's handle at call time that is stored, not
        the handle itself: relinking the facade afterwards leaves this index on
        the curve it was given, and a later link_to is how it moves."""
        ...
    def needs_forecast(self, fixing_date: Date) -> bool:
        """Whether fixing_date has to be forecast rather than read from
        history, decided against the latest period that could have been
        published by the settings' evaluation date."""
        ...
    def __repr__(self) -> str: ...
