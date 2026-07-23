# Hand-written stubs for itofin.indexes; sync manually with src/hullwhite.rs (#517).

from itofin import Settings
from itofin.termstructures import FlatForward

class Euribor:
    """The Euribor IBOR index family (only the 6-month tenor is wired)."""

    @staticmethod
    def six_months(curve: FlatForward, settings: Settings) -> Euribor: ...
