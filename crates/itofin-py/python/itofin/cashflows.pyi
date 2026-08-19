# Hand-written stubs for itofin.cashflows; sync manually with src/cashflows.rs
# (#517, #848, #863, #626).

from itofin.indexes import CpiInterpolationType, IborIndex, YoYInflationIndex
from itofin.termstructures import (
    ConstantYoYOptionletVolatility,
    YieldTermStructure,
)
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

class YoYInflationOptionletCouponPricer:
    """Values a capped or floored year-on-year coupon's optionlets off a
    volatility surface.

    The distribution is chosen by the constructor: black is lognormal,
    unit_displaced lognormal in 1 + rate and bachelier normal. The settings
    behind volatility and behind the priced coupons' index must be the same
    object. nominal_ts is optional: only the discounted price path reads it.
    """

    @staticmethod
    def black(
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure | None = None,
    ) -> YoYInflationOptionletCouponPricer: ...
    @staticmethod
    def unit_displaced(
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure | None = None,
    ) -> YoYInflationOptionletCouponPricer: ...
    @staticmethod
    def bachelier(
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure | None = None,
    ) -> YoYInflationOptionletCouponPricer: ...

class CappedFlooredYoYInflationCoupon:
    """A year-on-year inflation coupon with a cap and/or floor on its rate.

    Built only through YoYInflationLeg.capped_floored_coupons. A negative
    gearing swaps the two roles, so is_capped and effective_cap answer off the
    stored level rather than off what the leg was given.
    """

    def rate(self) -> float:
        """The swaplet rate plus the floorlet, less the caplet."""
        ...
    def amount(self) -> float:
        """rate() * accrual_period() * nominal()."""
        ...
    def is_capped(self) -> bool: ...
    def is_floored(self) -> bool: ...
    def effective_cap(self) -> float:
        """(cap - spread) / gearing, the strike the caplet is struck at."""
        ...
    def effective_floor(self) -> float:
        """(floor - spread) / gearing, the strike the floorlet is struck at."""
        ...

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
        caps: list[float] | None = None,
        floors: list[float] | None = None,
    ) -> None: ...
    def coupons(self) -> list[YoYInflationCoupon]:
        """The coupons, each carrying the default swaplet pricer.

        Rebuilt on every call, so bind the list once rather than calling this
        per read. Given caps or floors the coupons come back unpriced, and
        capped_floored_coupons is the intended entry.
        """
        ...
    def capped_floored_coupons(
        self, pricer: YoYInflationOptionletCouponPricer
    ) -> list[CappedFlooredYoYInflationCoupon]:
        """The coupons wrapped in the leg's per-coupon caps and floors, each
        carrying pricer.

        The pricer is required: a capped leg withholds the core's default
        swaplet pricer, and that pricer could not value the optionlets anyway.
        Rebuilt on every call, as coupons() is.
        """
        ...

class IborLeg:
    """Builds a sequence of floating ibor coupons from a schedule.

    The setters keep the core's fluent shape: each returns a NEW leg carrying
    the extra setting, so a leg bound to a name never changes under a later
    call. An unset optional leaves the core default in place: a Following
    payment roll and the index's own fixing days and day counter.

    The coupons themselves are not exposed: they are consumed by the raw
    CapFloor.cap / floor / collar constructors, which is the reason this leg
    exists. No caps/floors setter is offered either - a capped leg withholds the
    default coupon pricer in the core, so the strikes belong on the cap/floor
    constructor.
    """

    def __init__(self, schedule: Schedule, index: IborIndex) -> None: ...
    def with_notional(self, notional: float) -> IborLeg:
        """Required: a leg with no notional raises from coupon_count()."""
        ...
    def with_payment_day_counter(self, day_counter: DayCounter) -> IborLeg: ...
    def with_payment_adjustment(
        self, convention: BusinessDayConvention
    ) -> IborLeg: ...
    def with_fixing_days(self, fixing_days: int) -> IborLeg: ...
    def coupon_count(self) -> int:
        """The number of coupons a construction would produce, one per schedule
        period. Raises ItofinError with no notional set, or on a schedule
        holding fewer than two dates."""
        ...
