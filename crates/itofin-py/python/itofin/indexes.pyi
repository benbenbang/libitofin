# Hand-written stubs for itofin.indexes; sync manually with src/hullwhite.rs and src/helpers.rs (#517).

from itofin import Settings
from itofin.termstructures import YieldTermStructure
from itofin.time import Date, Period

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
