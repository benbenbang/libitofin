# Hand-written stubs for itofin.time; sync manually with src/time.rs (#517).

from typing import overload

def is_imm_date(date: Date, main_cycle: bool = False) -> bool:
    """Whether date is an IMM date.

    An IMM date is the third Wednesday of the month, and of March, June,
    September or December only when main_cycle is set.

    Args:
        date: The date to test.
        main_cycle: Restrict the test to the March/June/September/December
            cycle.

    Returns:
        True when date is an IMM date under the selected cycle.
    """
    ...

def next_imm_date(date: Date, main_cycle: bool = False) -> Date:
    """The next IMM date strictly following date.

    Args:
        date: The date to start from; the result is strictly after it.
        main_cycle: Restrict the result to the March/June/September/December
            cycle.

    Returns:
        The next IMM date under the selected cycle.
    """
    ...

class Date:
    """A calendar date with a validation guard.

    Every constructor and every arithmetic result is range-checked before the
    core is reached, so an out-of-range date is an error rather than a panic.
    """

    def __init__(self, day: int, month: int, year: int) -> None:
        """Build a date from its three components.

        Args:
            day: The day of the month, within the length of that month.
            month: The month, from 1 to 12.
            year: The year, from 1901 to 2199.

        Raises:
            ItofinError: If month is outside [1, 12], year is outside
                [1901, 2199], or day is outside the length of that month.
        """
        ...

    @property
    def year(self) -> int:
        """The year."""
        ...

    @property
    def month(self) -> int:
        """The month, from 1 to 12."""
        ...

    @property
    def day(self) -> int:
        """The day of the month."""
        ...

    def __add__(self, days: int) -> Date:
        """Shift the date forward by a number of calendar days.

        Args:
            days: The number of calendar days to add.

        Returns:
            The shifted date.

        Raises:
            ItofinError: If the result falls outside the representable date
                range.
        """
        ...

    @overload
    def __sub__(self, days: int) -> Date:
        """Shift the date back by a number of calendar days.

        Args:
            days: The number of calendar days to subtract.

        Returns:
            The shifted date.

        Raises:
            ItofinError: If the result falls outside the representable date
                range.
        """
        ...

    @overload
    def __sub__(self, other: Date) -> int:
        """The signed number of days from other to this date.

        Args:
            other: The date to measure from.

        Returns:
            The signed day count between the two dates.
        """
        ...

    def __eq__(self, other: object) -> bool:
        """Whether the two dates are the same calendar day.

        Args:
            other: The date to compare against.

        Returns:
            True when both stand for the same day.
        """
        ...

    def __repr__(self) -> str:
        """Return the constructor form of the date.

        Returns:
            The date as Date(day, month, year).
        """
        ...

class Period:
    """A signed length in one calendar unit (unit: Days, Weeks, Months, Years)."""

    def __init__(self, n: int, unit: str) -> None:
        """Build a period of n units.

        Args:
            n: The length, which may be negative.
            unit: One of "Days", "Weeks", "Months", "Years".

        Raises:
            ItofinError: If unit is not one of the four accepted strings.
        """
        ...

    def __eq__(self, other: object) -> bool:
        """Semantic equality: 7 Days equals 1 Week and 12 Months equals 1 Year,
        while an undecidable pair such as 30 Days against 1 Month is not equal.

        Args:
            other: The period to compare against.

        Returns:
            True when the two lengths are decidably the same.
        """
        ...

    def __hash__(self) -> int:
        """Hashes the canonical form, so equal periods hash equal.

        Normalizing collapses 7 Days onto 1 Week, 12 Months onto 1 Year and
        every zero length onto 0 Days before the hash is taken.

        Returns:
            The hash of the normalized length and unit.
        """
        ...

    def __repr__(self) -> str:
        """Return the constructor form of the period.

        Returns:
            The period as Period(length, unit).
        """
        ...

class Calendar:
    """A business-day calendar."""

    @staticmethod
    def target() -> Calendar:
        """The TARGET calendar.

        Returns:
            The TARGET business-day calendar.
        """
        ...

    @staticmethod
    def null_calendar() -> Calendar:
        """The calendar holding no holidays at all.

        Returns:
            The null calendar, on which every day is a business day.
        """
        ...

    @staticmethod
    def weekends_only() -> Calendar:
        """The weekends-only calendar: Saturdays and Sundays are holidays and no
        other day is. The calendar the ISDA CDS conventions roll on.

        It is not substitutable by the null calendar, which holds no holidays at
        all, nor by a national calendar, which adds public holidays.

        Returns:
            The weekends-only calendar.
        """
        ...

    @staticmethod
    def united_kingdom() -> Calendar:
        """The UK settlement calendar. Only the Settlement market is exposed:
        the Exchange and Metals markets share an identical business-day rule in
        the core and differ solely in their name.

        Returns:
            The UK settlement calendar.
        """
        ...

    def adjust(self, date: Date, convention: BusinessDayConvention) -> Date:
        """Roll a date to the nearest business day.

        Args:
            date: The date to roll.
            convention: The rolling rule to apply.

        Returns:
            The adjusted date, unchanged when it is already a business day.
        """
        ...

    def advance(
        self,
        date: Date,
        n: int,
        unit: str,
        convention: BusinessDayConvention,
        end_of_month: bool,
    ) -> Date:
        """Advance a date by n units and adjust the result.

        Args:
            date: The date to advance from.
            n: The number of units to advance, which may be negative.
            unit: One of "Days", "Weeks", "Months", "Years".
            convention: The rule the advanced date is rolled under.
            end_of_month: Keep the result on the month end when the starting
                date is one.

        Returns:
            The advanced and adjusted date.

        Raises:
            ItofinError: If unit is not one of the four accepted strings.
        """
        ...

    def __repr__(self) -> str:
        """Return the calendar and its name.

        Returns:
            The calendar as Calendar(name).
        """
        ...

class DayCounter:
    """A year-fraction convention."""

    @staticmethod
    def actual360() -> DayCounter:
        """The Actual/360 convention.

        Returns:
            An Actual/360 day counter.
        """
        ...

    @staticmethod
    def actual365_fixed() -> DayCounter:
        """The Actual/365 (Fixed) convention.

        Returns:
            An Actual/365 (Fixed) day counter.
        """
        ...

    @staticmethod
    def actual_actual_isda() -> DayCounter:
        """The Actual/Actual (ISDA) convention.

        Returns:
            An Actual/Actual day counter on the ISDA convention.
        """
        ...

    @staticmethod
    def thirty360_bond_basis() -> DayCounter:
        """The 30/360 (Bond Basis) convention.

        Returns:
            A 30/360 day counter on the bond-basis convention.
        """
        ...

    def year_fraction(self, d1: Date, d2: Date) -> float:
        """The period [d1, d2] as a fraction of a year under this convention.

        Args:
            d1: The start of the period.
            d2: The end of the period.

        Returns:
            The year fraction between the two dates.
        """
        ...

    def __eq__(self, other: object) -> bool:
        """Equality by convention name, so two independently built Actual360s
        are equal.

        Args:
            other: The day counter to compare against.

        Returns:
            True when both carry the same convention name.
        """
        ...

    def __hash__(self) -> int:
        """Hashes the convention name, the field equality compares.

        Returns:
            The hash of the convention name, so equal day counters hash equal.
        """
        ...

    def __repr__(self) -> str:
        """Return the day counter and its convention name.

        Returns:
            The day counter as DayCounter(name).
        """
        ...

class Frequency:
    """A coupon or fixing frequency.

    Only the variants the ported fixtures use are surfaced; new ones are
    appended, so the integer values of the existing variants are unchanged.
    """

    Annual: Frequency
    Semiannual: Frequency
    Quarterly: Frequency
    Monthly: Frequency

class BusinessDayConvention:
    """A holiday-rolling rule. Every core variant is covered; the four listed
    last are appended, so the integer values of the first three are unchanged."""

    ModifiedFollowing: BusinessDayConvention
    Following: BusinessDayConvention
    Unadjusted: BusinessDayConvention
    Preceding: BusinessDayConvention
    ModifiedPreceding: BusinessDayConvention
    HalfMonthModifiedFollowing: BusinessDayConvention
    Nearest: BusinessDayConvention

class DateGeneration:
    """The rule a Schedule generates its dates by.

    Backward and Forward roll from one end of the range to the other; Zero keeps
    only the two endpoints; the ThirdWednesday and Twentieth families snap the
    interior dates onto an IMM Wednesday or the twentieth of the month. A
    Schedule builds under the three CDS rules, but SpreadCdsHelper rejects them:
    their maturity comes from a core routine that is not ported yet.
    """

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
    ) -> None:
        """Build the schedule.

        Args:
            start: The effective date, which must be strictly before end.
            end: The termination date.
            frequency: The coupon frequency the interior dates are spaced at.
            calendar: The calendar the dates roll on.
            convention: The rule every date but the last is rolled under.
            rule: The date-generation rule; defaults to Forward.
            termination_convention: The rule the last date is rolled under;
                None applies convention to it as well.

        Raises:
            ItofinError: If start is not strictly before end.
        """
        ...

    def size(self) -> int:
        """The number of dates in the schedule.

        Returns:
            The date count, one more than the number of periods.
        """
        ...

    def date(self, i: int) -> Date:
        """The i-th date in the schedule.

        Args:
            i: The zero-based index into the dates.

        Returns:
            The date at that index.

        Raises:
            ItofinError: If i is out of range.
        """
        ...

    def dates(self) -> list[Date]:
        """All the schedule dates.

        Returns:
            The dates in order, from the effective date to the termination date.
        """
        ...
