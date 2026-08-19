# Hand-written stubs for itofin.time; sync manually with src/time.rs (#517).

def is_imm_date(date: Date, main_cycle: bool = False) -> bool:
    """Whether date is an IMM date: the third Wednesday of the month, and of
    March, June, September or December only when main_cycle is set."""
    ...

def next_imm_date(date: Date, main_cycle: bool = False) -> Date:
    """The next IMM date strictly following date, restricted to the March/June/
    September/December cycle when main_cycle is set."""
    ...

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
    def __eq__(self, other: object) -> bool:
        """Semantic equality: 7 Days equals 1 Week and 12 Months equals 1 Year,
        while an undecidable pair such as 30 Days against 1 Month is not equal."""
        ...

    def __hash__(self) -> int:
        """Hashes the canonical form, so equal periods hash equal."""
        ...

    def __repr__(self) -> str: ...

class Calendar:
    """A business-day calendar."""

    @staticmethod
    def target() -> Calendar: ...
    @staticmethod
    def null_calendar() -> Calendar: ...
    @staticmethod
    def weekends_only() -> Calendar:
        """The weekends-only calendar: Saturdays and Sundays are holidays and no
        other day is. The calendar the ISDA CDS conventions roll on."""
        ...
    @staticmethod
    def united_kingdom() -> Calendar:
        """The UK settlement calendar. Only the Settlement market is exposed:
        the Exchange and Metals markets share an identical business-day rule in
        the core and differ solely in their name."""
        ...
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
    def __eq__(self, other: object) -> bool:
        """Equality by convention name, so two independently built Actual360s
        are equal."""
        ...

    def __hash__(self) -> int:
        """Hashes the convention name, the field equality compares."""
        ...

    def __repr__(self) -> str: ...

class Frequency:
    """A coupon or fixing frequency."""

    Annual: Frequency
    Semiannual: Frequency
    Quarterly: Frequency
    Monthly: Frequency

class BusinessDayConvention:
    """A holiday-rolling rule."""

    ModifiedFollowing: BusinessDayConvention
    Following: BusinessDayConvention
    Unadjusted: BusinessDayConvention

class DateGeneration:
    """The rule a Schedule generates its dates by."""

    Backward: DateGeneration
    Forward: DateGeneration
    Zero: DateGeneration
    ThirdWednesday: DateGeneration
    ThirdWednesdayInclusive: DateGeneration
    Twentieth: DateGeneration
    TwentiethIMM: DateGeneration
    OldCDS: DateGeneration
    CDS: DateGeneration
    CDS2015: DateGeneration

class Schedule:
    """A sequence of coupon dates built through MakeSchedule.

    termination_convention rolls the last date only, and defaults to
    convention. CDS conventions need the two to differ: a credit helper leaves
    its maturity unadjusted while paying Following."""

    def __init__(
        self,
        start: Date,
        end: Date,
        frequency: Frequency,
        calendar: Calendar,
        convention: BusinessDayConvention,
        rule: DateGeneration = ...,
        termination_convention: BusinessDayConvention | None = None,
    ) -> None: ...
    def size(self) -> int: ...
    def date(self, i: int) -> Date: ...
    def dates(self) -> list[Date]: ...
