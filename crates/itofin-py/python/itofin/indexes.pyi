# Hand-written stubs for itofin.indexes; sync manually with src/hullwhite.rs (#517).

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
