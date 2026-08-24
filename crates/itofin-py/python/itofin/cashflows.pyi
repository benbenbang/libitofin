# Hand-written stubs for itofin.cashflows; sync manually with src/cashflows.rs
# (#517, #848, #863, #626, #878).

# itofin library
from itofin import Settings
from itofin.indexes import CpiInterpolationType, IborIndex, YoYInflationIndex
from itofin.termstructures import ConstantYoYOptionletVolatility, YieldTermStructure
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period, Schedule

class YoYInflationCoupon:
    """One coupon of a year-on-year inflation leg.

    Built only through YoYInflationLeg, which attaches the pricer rate() and
    amount() need.
    """

    def rate(self) -> float:
        """Return the rate the coupon accrues at: the geared index fixing plus the spread.

        Returns:
            float: The pricer's swaplet rate.

        Raises:
            ItofinError: If no pricer is attached, or resolving the fixing
                fails - a missing history entry, or a forecast off an index with
                no curve linked.
        """
        ...
    def amount(self) -> float:
        """Return what the coupon pays on its payment date, undiscounted.

        Returns:
            float: rate() * accrual_period() * nominal().

        Raises:
            ItofinError: As rate().
        """
        ...
    def fixing_date(self) -> Date:
        """Return the date the observation is published on.

        The reference-period end moved back by the observation lag, then back
        the fixing days. This is not the date the rate resolves at: an inflation
        index has no fixing calendar, so with no fixing days the roll is inert.

        Returns:
            Date: The publication date.
        """
        ...
    def index_fixing(self) -> float:
        """Return the year-on-year rate observed, before gearing and spread.

        It lags off accrual_end_date(), not off fixing_date(): a year-on-year
        coupon overrides the base rule that reads the index at its fixing date.

        Returns:
            float: The observed rate.

        Raises:
            ItofinError: As rate(), bar the missing pricer.
        """
        ...
    def nominal(self) -> float:
        """Return the nominal the coupon accrues on.

        Returns:
            float: The nominal.
        """
        ...
    def accrual_start_date(self) -> Date:
        """Return the start of the accrual period.

        Returns:
            Date: The accrual start.
        """
        ...
    def accrual_end_date(self) -> Date:
        """Return the end of the accrual period, which is also where the observation lags from.

        Returns:
            Date: The accrual end.
        """
        ...
    def accrual_period(self) -> float:
        """Return the whole accrual period as a fraction of a year.

        Measured with day_counter() over the reference period.

        Returns:
            float: The year fraction.
        """
        ...
    def date(self) -> Date:
        """Return the payment date: the accrual end rolled on the leg's payment calendar.

        Returns:
            Date: The payment date.
        """
        ...
    def day_counter(self) -> DayCounter:
        """Return the day counter the accrual is measured with.

        Returns:
            DayCounter: The coupon day count.
        """
        ...
    def gearing(self) -> float:
        """Return the multiplicative coefficient applied to the index fixing.

        Returns:
            float: The gearing.
        """
        ...
    def spread(self) -> float:
        """Return the spread paid over the geared fixing.

        Returns:
            float: The spread.
        """
        ...
    def observation_lag(self) -> Period:
        """Return how far back the coupon observes the index.

        Returns:
            Period: The observation lag.
        """
        ...
    def interpolation(self) -> CpiInterpolationType:
        """Return how the observation interpolates between index fixings.

        Returns:
            CpiInterpolationType: Flat or Linear.
        """
        ...
    def fixing_days(self) -> int:
        """Return the number of business days the fixing date rolls back by.

        Returns:
            int: The fixing days.
        """
        ...

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
    ) -> YoYInflationOptionletCouponPricer:
        """Build a pricer valuing optionlets under the lognormal model.

        Args:
            volatility (ConstantYoYOptionletVolatility): The surface optionlet
                volatilities are read off.
            nominal_ts (YieldTermStructure | None): The discount curve; only the
                discounted price path reads it.

        Returns:
            YoYInflationOptionletCouponPricer: The lognormal pricer.
        """
        ...
    @staticmethod
    def unit_displaced(
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure | None = None,
    ) -> YoYInflationOptionletCouponPricer:
        """Build a pricer valuing optionlets under the unit-displaced lognormal model.

        Lognormal in 1 + rate, the usual quoting convention for an inflation
        rate that may go negative.

        Args:
            volatility (ConstantYoYOptionletVolatility): The surface optionlet
                volatilities are read off.
            nominal_ts (YieldTermStructure | None): The discount curve; only the
                discounted price path reads it.

        Returns:
            YoYInflationOptionletCouponPricer: The unit-displaced pricer.
        """
        ...
    @staticmethod
    def bachelier(
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure | None = None,
    ) -> YoYInflationOptionletCouponPricer:
        """Build a pricer valuing optionlets under the normal model.

        Args:
            volatility (ConstantYoYOptionletVolatility): The surface optionlet
                volatilities are read off.
            nominal_ts (YieldTermStructure | None): The discount curve; only the
                discounted price path reads it.

        Returns:
            YoYInflationOptionletCouponPricer: The normal pricer.
        """
        ...

class CappedFlooredYoYInflationCoupon:
    """A year-on-year inflation coupon with a cap and/or floor on its rate.

    Built only through YoYInflationLeg.capped_floored_coupons. A negative
    gearing swaps the two roles, so is_capped and effective_cap answer off the
    stored level rather than off what the leg was given.
    """

    def rate(self) -> float:
        """Return the rate the coupon accrues at.

        The underlying's swaplet rate plus the floorlet, less the caplet.

        Returns:
            float: The capped and floored rate.

        Raises:
            ItofinError: If no pricer is attached, if resolving the fixing
                fails, or if the surface refuses the volatility - a strike
                outside its domain, or an observation before its base date.
        """
        ...
    def amount(self) -> float:
        """Return what the coupon pays on its payment date, undiscounted.

        Returns:
            float: rate() * accrual_period() * nominal().

        Raises:
            ItofinError: As rate().
        """
        ...
    def is_capped(self) -> bool:
        """Return whether a cap applies.

        Returns:
            bool: True if the stored cap level is set.
        """
        ...
    def is_floored(self) -> bool:
        """Return whether a floor applies.

        Returns:
            bool: True if the stored floor level is set.
        """
        ...
    def effective_cap(self) -> float:
        """Return the de-spread, de-geared cap the caplet is struck at.

        Read off the stored level, so a negative gearing has already swapped the
        two roles.

        Returns:
            float: (cap - spread) / gearing.
        """
        ...
    def effective_floor(self) -> float:
        """Return the de-spread, de-geared floor the floorlet is struck at.

        Read off the stored level, so a negative gearing has already swapped the
        two roles.

        Returns:
            float: (floor - spread) / gearing.
        """
        ...

class YoYInflationLeg:
    """Builds a sequence of year-on-year inflation coupons from a schedule.

    The core builder is a consumed-self fluent chain, which does not cross the
    FFI boundary; this facade takes the whole configuration up front and
    assembles the chain inside coupons(). An unset optional leaves the core
    default in place: a ModifiedFollowing payment roll, no fixing days, a unit
    gearing and no spread.

    The caps and floors lists select which of the two coupon types the leg
    produces: given either, coupons() hands back coupons the core deliberately
    leaves unpriced and capped_floored_coupons is the intended entry.
    """

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
    ) -> None:
        """Configure a leg over schedule paying index, observed observation_lag back.

        payment_day_counter is required here although the core takes it through
        a setter, so a missing one is a build-time error rather than one raised
        from coupons(). A notional is just as required by the core but stays
        optional, since the per-coupon notionals list is the other way to supply
        it; giving neither surfaces that error from coupons().

        Args:
            schedule (Schedule): The accrual schedule, one coupon per period.
            payment_calendar (Calendar): The calendar payment dates roll on.
            index (YoYInflationIndex): The index the coupons observe.
            observation_lag (Period): How far back each coupon observes it.
            interpolation (CpiInterpolationType): How the observation
                interpolates between index fixings.
            payment_day_counter (DayCounter): The day count the accruals are
                measured with.
            notional (float | None): One nominal for every coupon.
            notionals (list[float] | None): A per-coupon nominal, the
                alternative to notional.
            payment_adjustment (BusinessDayConvention | None): The payment roll;
                the core default is ModifiedFollowing.
            fixing_days (int | None): The business days the fixing date rolls
                back by; the core default is none.
            gearing (float | None): One gearing for every coupon; the core
                default is unit.
            gearings (list[float] | None): A per-coupon gearing.
            spread (float | None): One spread for every coupon; the core default
                is none.
            spreads (list[float] | None): A per-coupon spread.
            caps (list[float] | None): A per-coupon cap level; given either
                list, capped_floored_coupons is the intended entry.
            floors (list[float] | None): A per-coupon floor level.
        """
        ...
    def coupons(self) -> list[YoYInflationCoupon]:
        """Return the coupons, each carrying the default swaplet pricer.

        Every call rebuilds the leg, so the coupons handed back are fresh
        objects each time: bind the list once rather than calling this per read,
        or two reads compare different objects. Given caps or floors the coupons
        come back unpriced, and capped_floored_coupons is the intended entry.

        Returns:
            list[YoYInflationCoupon]: The freshly built coupons.

        Raises:
            ItofinError: If the leg has no notional, the schedule holds fewer
                than two dates, or there are more notionals, gearings or spreads
                than the schedule has periods.
        """
        ...
    def capped_floored_coupons(
        self, pricer: YoYInflationOptionletCouponPricer
    ) -> list[CappedFlooredYoYInflationCoupon]:
        """Return the coupons wrapped in the leg's caps and floors, each carrying pricer.

        The pricer is required rather than optional: the core withholds its
        default swaplet pricer from a capped leg, and a swaplet pricer could not
        value the optionlets anyway. One pricer is installed across every
        coupon. Rebuilt on every call, as coupons() is.

        Args:
            pricer (YoYInflationOptionletCouponPricer): The pricer installed
                across every coupon.

        Returns:
            list[CappedFlooredYoYInflationCoupon]: The freshly built coupons.

        Raises:
            ItofinError: As coupons(), plus more caps or floors than the
                schedule has periods, and a cap sitting below its floor.
        """
        ...
    def build(self) -> Leg:
        """Return the leg with its coupon type erased, the form npv() sums.

        The plain path erases coupons already carrying the default swaplet
        pricer. With a caps or floors list the erased coupons carry NO pricer,
        and because every call rebuilds the leg a pricer installed through
        capped_floored_coupons() does not reach them: a capped erased leg
        reports "pricer not set" from CashFlow.amount(), and the priced capped
        path stays capped_floored_coupons(). Rebuilt on every call.

        Returns:
            Leg: The freshly built erased leg.

        Raises:
            ItofinError: As coupons().
        """
        ...

class CashFlow:
    """One erased flow of a Leg, read-only.

    It answers what it pays and when, which is all the leg-summing npv() needs;
    the concrete coupon accessors stay on the typed coupon wrappers."""

    def amount(self) -> float:
        """Return what the flow pays on its date, undiscounted.

        Returns:
            float: The undiscounted payment amount.

        Raises:
            ItofinError: On a coupon with no pricer attached, and on whatever
                resolving its fixing reports - a missing history entry, or a
                forecast off an index with no curve linked.
        """
        ...
    def date(self) -> Date:
        """Return the date the flow pays on.

        Returns:
            Date: The payment date.
        """
        ...

class Leg:
    """A sequence of erased cash flows, built by a leg builder's build().

    Indexable and sized, which with CashFlow's two accessors is enough to
    hand-check what npv() sums."""

    def __len__(self) -> int:
        """Return the number of flows on the leg.

        Returns:
            int: The flow count.
        """
        ...
    def __getitem__(self, index: int) -> CashFlow:
        """Return the flow at index, counting from the end when negative.

        Args:
            index (int): The position, negative to count from the end.

        Returns:
            CashFlow: The flow at that position.

        Raises:
            IndexError: If index is out of range.
        """
        ...

def npv(
    leg: Leg,
    discount_curve: YieldTermStructure,
    settings: Settings,
    include_settlement_date_flows: bool | None = None,
    settlement_date: Date | None = None,
    npv_date: Date | None = None,
) -> float:
    """Return the NPV of leg: every surviving flow discounted on discount_curve.

    Args:
        leg (Leg): The erased flows to sum.
        discount_curve (YieldTermStructure): The curve the flows discount on.
        settings (Settings): The evaluation context deciding which flows have
            occurred.
        include_settlement_date_flows (bool | None): Whether a flow paying
            exactly on the settlement date counts; None defers to the settings'
            include_todays_cash_flows policy.
        settlement_date (Date | None): The date deciding which flows have
            occurred; None uses the evaluation date, which must then be set.
        npv_date (Date | None): The date the sum is discounted to; None uses
            settlement_date.

    Returns:
        float: The discounted sum; exactly 0.0 for an empty leg.

    Raises:
        ItofinError: On a flow or curve lookup failure, and without a
            settlement_date when the evaluation date is unset.
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

    def __init__(self, schedule: Schedule, index: IborIndex) -> None:
        """Configure a leg over schedule paying index, on the schedule's own calendar.

        Args:
            schedule (Schedule): The accrual schedule, one coupon per period.
            index (IborIndex): The index the floating coupons fix off.
        """
        ...
    def with_notional(self, notional: float) -> IborLeg:
        """Return the leg with notional on every coupon.

        Required: a leg built without one reports "no notional given" from
        coupon_count().

        Args:
            notional (float): The nominal every coupon accrues on.

        Returns:
            IborLeg: A new leg carrying the notional.
        """
        ...
    def with_payment_day_counter(self, day_counter: DayCounter) -> IborLeg:
        """Return the leg accruing with day_counter, overriding the index's.

        Args:
            day_counter (DayCounter): The day count the accruals are measured
                with.

        Returns:
            IborLeg: A new leg carrying the day counter.
        """
        ...
    def with_payment_adjustment(self, convention: BusinessDayConvention) -> IborLeg:
        """Return the leg rolling its payment dates with convention.

        Args:
            convention (BusinessDayConvention): The payment roll, overriding the
                core default of Following.

        Returns:
            IborLeg: A new leg carrying the convention.
        """
        ...
    def with_fixing_days(self, fixing_days: int) -> IborLeg:
        """Return the leg fixing fixing_days business days before each accrual start.

        Overrides the index's own count.

        Args:
            fixing_days (int): The business days each coupon fixes ahead of its
                accrual start.

        Returns:
            IborLeg: A new leg carrying the fixing days.
        """
        ...
    def coupon_count(self) -> int:
        """Return the number of coupons the leg builds, one per schedule period.

        The leg is rebuilt on every call, here and in the cap/floor
        constructors, so this counts the coupons a construction would produce
        rather than a stored list.

        Returns:
            int: The coupon count.

        Raises:
            ItofinError: If the leg has no notional, the schedule holds fewer
                than two dates, or a coupon's own preconditions reject.
        """
        ...
