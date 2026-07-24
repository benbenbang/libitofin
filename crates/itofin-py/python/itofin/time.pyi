# Hand-written stubs for itofin.time; sync manually with src/time.rs (#517).

class Date:
    """A calendar date with a validation guard."""

    def __init__(self, day: int, month: int, year: int) -> None: ...
    @property
    def year(self) -> int: ...
    @property
    def month(self) -> int: ...
    @property
    def day(self) -> int: ...
    def __add__(self, days: int) -> Date: ...
    def __sub__(self, days: int) -> Date: ...
    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

class Period:
    """A signed length in one calendar unit (unit: Days, Weeks, Months, Years)."""

    def __init__(self, n: int, unit: str) -> None: ...
    def __repr__(self) -> str: ...

class Calendar:
    """A business-day calendar."""

    @staticmethod
    def target() -> Calendar: ...
    @staticmethod
    def null_calendar() -> Calendar: ...
    def adjust(self, date: Date, convention: BusinessDayConvention) -> Date: ...
    def advance(
        self,
        date: Date,
        n: int,
        unit: str,
        convention: BusinessDayConvention,
        end_of_month: bool,
    ) -> Date: ...
    def __repr__(self) -> str: ...

class DayCounter:
    """A year-fraction convention."""

    @staticmethod
    def actual360() -> DayCounter: ...
    @staticmethod
    def actual365_fixed() -> DayCounter: ...
    @staticmethod
    def actual_actual_isda() -> DayCounter: ...
    @staticmethod
    def thirty360_bond_basis() -> DayCounter: ...
    def __repr__(self) -> str: ...

class Frequency:
    """A coupon frequency."""

    Annual: Frequency
    Semiannual: Frequency

class BusinessDayConvention:
    """A holiday-rolling rule."""

    ModifiedFollowing: BusinessDayConvention
    Following: BusinessDayConvention
    Unadjusted: BusinessDayConvention

class Schedule:
    """A sequence of coupon dates built through MakeSchedule."""

    def __init__(
        self,
        start: Date,
        end: Date,
        frequency: Frequency,
        calendar: Calendar,
        convention: BusinessDayConvention,
    ) -> None: ...
    def size(self) -> int: ...
    def date(self, i: int) -> Date: ...
    def dates(self) -> list[Date]: ...
