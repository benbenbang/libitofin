# Hand-written stubs for itofin.termstructures; sync manually with src/curve.rs (#517).

from itofin.time import Date, DayCounter

class FlatForward:
    """A flat continuously-compounded yield curve behind a Handle."""

    def __init__(self, reference_date: Date, rate: float, day_counter: DayCounter) -> None: ...
    def discount(self, t: float) -> float: ...
    def zero_rate(self, t: float) -> float: ...
