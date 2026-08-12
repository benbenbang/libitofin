# Hand-written stubs for itofin.cashflows; sync manually with src/cashflows.rs
# (#517, #848).

from itofin.indexes import CpiInterpolationType, YoYInflationIndex
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Period,
    Schedule,
)

class YoYInflationCoupon:
    """One coupon of a year-on-year inflation leg.

    Built only through YoYInflationLeg, which attaches the pricer rate() and
    amount() need.
    """

    def rate(self) -> float:
        """The geared index fixing plus the spread; needs a pricer."""
        ...
    def amount(self) -> float:
        """rate() * accrual_period() * nominal()."""
        ...
    def fixing_date(self) -> Date:
        """The observation publication date, not where the rate resolves."""
        ...
    def index_fixing(self) -> float:
        """The year-on-year rate observed, lagged off the accrual end."""
        ...
    def nominal(self) -> float: ...
    def accrual_start_date(self) -> Date: ...
    def accrual_end_date(self) -> Date: ...
    def accrual_period(self) -> float: ...
    def date(self) -> Date:
        """The payment date."""
        ...
    def day_counter(self) -> DayCounter: ...
    def gearing(self) -> float: ...
    def spread(self) -> float: ...
    def observation_lag(self) -> Period: ...
    def interpolation(self) -> CpiInterpolationType: ...
    def fixing_days(self) -> int: ...

class YoYInflationLeg:
    """Builds a sequence of year-on-year inflation coupons from a schedule."""

    def __init__(
        self,
        schedule: Schedule,
        payment_calendar: Calendar,
        index: YoYInflationIndex,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
        payment_day_counter: DayCounter,
        notional: float | None = None,
        notionals: list[float] | None = None,
        payment_adjustment: BusinessDayConvention | None = None,
        fixing_days: int | None = None,
        gearing: float | None = None,
        gearings: list[float] | None = None,
        spread: float | None = None,
        spreads: list[float] | None = None,
    ) -> None: ...
    def coupons(self) -> list[YoYInflationCoupon]:
        """The coupons, each carrying the default swaplet pricer.

        Rebuilt on every call, so bind the list once rather than calling this
        per read.
        """
        ...
