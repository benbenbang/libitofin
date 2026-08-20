# Hand-written stubs for itofin.termstructures; sync manually with src/curve.rs, src/vol.rs,
# src/helpers.rs, src/swaptionvol.rs, src/optionletvol.rs, src/smilesection.rs and
# src/credit.rs, src/credithelpers.rs and src/inflation.rs (#517).

from itofin import Settings
from itofin.indexes import (
    CpiInterpolationType,
    IborIndex,
    OvernightIndex,
    SwapIndex,
    YoYInflationIndex,
    ZeroInflationIndex,
)
from itofin.quotes import SimpleQuote
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DateGeneration,
    DayCounter,
    Frequency,
    Period,
)

class YieldTermStructure:
    """Shared base for every yield curve: discount factors, zero and forward rates.

    Concrete curves subclass this and supply only their constructor; the whole
    query surface below is inherited.
    """

    def discount(self, t: float, extrapolate: bool = False) -> float:
        """Return the discount factor at year-fraction t.

        Args:
            t (float): The year fraction, in the curve's own day count.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The discount factor.

        Raises:
            ItofinError: If t is past the curve's range and neither extrapolate
                nor the curve's own extrapolation flag allows it.
        """
        ...
    def discount_date(self, date: Date, extrapolate: bool = False) -> float:
        """Return the discount factor from date back to the reference date.

        Args:
            date (Date): The date discounted from.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The discount factor.

        Raises:
            ItofinError: If date is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def zero_rate(self, t: float, extrapolate: bool = False) -> float:
        """Return the continuously-compounded zero rate at year-fraction t.

        Args:
            t (float): The year fraction, in the curve's own day count.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The zero rate, continuously compounded at annual frequency.

        Raises:
            ItofinError: If t is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def forward_rate(self, t1: float, t2: float, extrapolate: bool = False) -> float:
        """Return the continuously-compounded forward rate between t1 and t2.

        Args:
            t1 (float): The start year fraction.
            t2 (float): The end year fraction.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The forward rate, continuously compounded at annual
                frequency.

        Raises:
            ItofinError: If either time is past the curve's range and
                extrapolation is not allowed.
        """
        ...
    def reference_date(self) -> Date:
        """Return the date at which the discount factor is 1.0.

        Returns:
            Date: The curve's reference date.

        Raises:
            ItofinError: On a curve whose reference date moves with an
                evaluation date that is not set.
        """
        ...
    def max_date(self) -> Date:
        """Return the latest date for which the curve can return values.

        Returns:
            Date: The curve's maximum date.
        """
        ...
    def allows_extrapolation(self) -> bool:
        """Return whether the curve answers dates and times beyond its maximum.

        Returns:
            bool: True when extrapolation is enabled on the curve itself.
        """
        ...
    def enable_extrapolation(self) -> None:
        """Allow extrapolation past the maximum date and time."""
        ...
    def disable_extrapolation(self) -> None:
        """Forbid extrapolation past the maximum date and time."""
        ...

class BlackVolTermStructure:
    """Shared base for every Black-volatility surface: spot and forward vol/variance.

    Concrete surfaces subclass this and supply only their constructor; the
    whole query surface below is inherited, along with the strike domain and
    the extrapolation toggles.
    """

    def black_vol(self, t: float, strike: float, extrapolate: bool = False) -> float:
        """Return the spot Black volatility at year-fraction t and strike.

        Args:
            t (float): The year fraction, in the surface's own day count.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The Black volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def black_vol_date(self, date: Date, strike: float, extrapolate: bool = False) -> float:
        """Return the spot Black volatility at date and strike.

        Args:
            date (Date): The expiry the volatility is read at.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The Black volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def black_variance(self, t: float, strike: float, extrapolate: bool = False) -> float:
        """Return the spot Black variance at year-fraction t and strike.

        Args:
            t (float): The year fraction, in the surface's own day count.
            strike (float): The strike the variance is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The Black variance.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def black_variance_date(self, date: Date, strike: float, extrapolate: bool = False) -> float:
        """Return the spot Black variance at date and strike.

        Args:
            date (Date): The expiry the variance is read at.
            strike (float): The strike the variance is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The Black variance.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def black_forward_vol(self, t1: float, t2: float, strike: float, extrapolate: bool = False) -> float:
        """Return the forward Black volatility between year-fractions t1 and t2.

        Args:
            t1 (float): The start year fraction.
            t2 (float): The end year fraction.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The forward Black volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def black_forward_variance(self, t1: float, t2: float, strike: float, extrapolate: bool = False) -> float:
        """Return the forward Black variance between year-fractions t1 and t2.

        Args:
            t1 (float): The start year fraction.
            t2 (float): The end year fraction.
            strike (float): The strike the variance is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The forward Black variance.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def min_strike(self) -> float:
        """Return the minimum strike for which the surface can return volatilities.

        Returns:
            float: The lower bound of the strike domain.
        """
        ...
    def max_strike(self) -> float:
        """Return the maximum strike for which the surface can return volatilities.

        Returns:
            float: The upper bound of the strike domain.
        """
        ...
    def max_date(self) -> Date:
        """Return the latest date for which the surface can return values.

        Returns:
            Date: The surface's maximum date.
        """
        ...
    def allows_extrapolation(self) -> bool:
        """Return whether the surface answers dates and times beyond its maximum.

        Returns:
            bool: True when extrapolation is enabled on the surface itself.
        """
        ...
    def enable_extrapolation(self) -> None:
        """Allow extrapolation past the maximum date and time."""
        ...
    def disable_extrapolation(self) -> None:
        """Forbid extrapolation past the maximum date and time."""
        ...

class FlatForward(YieldTermStructure):
    """A flat continuously-compounded yield curve behind a Handle.

    Built at annual frequency with continuous compounding, the convention every
    downstream Heston and Hull-White oracle assumes.
    """

    def __init__(self, reference_date: Date, rate: float, day_counter: DayCounter) -> None:
        """Build the flat curve.

        Args:
            reference_date (Date): The date at which the discount factor is
                1.0.
            rate (float): The flat rate, continuously compounded at annual
                frequency.
            day_counter (DayCounter): The day count times are measured in.
        """
        ...

class ZeroCurve(YieldTermStructure):
    """A yield curve interpolating continuously-compounded zero rates between nodes.

    The first date is the reference date. Finite in time: queries past the last
    node require enable_extrapolation() or extrapolate=True.
    """

    def __init__(
        self,
        dates: list[Date],
        yields: list[float],
        day_counter: DayCounter,
        interpolation: str = "Linear",
    ) -> None:
        """Build the curve over its (date, zero-rate) nodes.

        Args:
            dates (list[Date]): The node dates, the first being the reference
                date.
            yields (list[float]): The continuously-compounded zero rate at each
                node.
            day_counter (DayCounter): The day count turning dates into times.
            interpolation (str): "Linear", the shipped behaviour, or "Cubic",
                the Kruger cubic factory, which is non-monotonic.

        Raises:
            ItofinError: On an unknown interpolation name, and on whatever the
                core rejects about the nodes.
        """
        ...

class DiscountCurve(YieldTermStructure):
    """A yield curve interpolating discount factors between nodes.

    The first date is the reference date and its discount must be 1.0. Finite
    in time: queries past the last node require extrapolation.
    """

    def __init__(
        self,
        dates: list[Date],
        discounts: list[float],
        day_counter: DayCounter,
        calendar: Calendar | None = None,
        interpolation: str = "LogLinear",
    ) -> None:
        """Build the curve over its (date, discount-factor) nodes.

        Args:
            dates (list[Date]): The node dates, the first being the reference
                date.
            discounts (list[float]): The discount factor at each node; the
                first must be 1.0.
            day_counter (DayCounter): The day count turning dates into times.
            calendar (Calendar | None): The curve's calendar; unlike the other
                two node curves this constructor accepts one.
            interpolation (str): "LogLinear", the shipped behaviour, giving
                piecewise-constant forwards, or "Cubic", which is
                non-monotonic.

        Raises:
            ItofinError: On an unknown interpolation name, and on whatever the
                core rejects about the nodes.
        """
        ...

class ForwardCurve(YieldTermStructure):
    """A yield curve interpolating instantaneous forward rates backward-flat.

    The first date is the reference date. Finite in time. Unlike ZeroCurve and
    DiscountCurve this curve offers no cubic option, QuantLib-SWIG exposing its
    cubic curve on the zero and discount curves only.
    """

    def __init__(
        self,
        dates: list[Date],
        forwards: list[float],
        day_counter: DayCounter,
    ) -> None:
        """Build the curve over its (date, forward-rate) nodes.

        Args:
            dates (list[Date]): The node dates, the first being the reference
                date.
            forwards (list[float]): The instantaneous forward rate at each
                node.
            day_counter (DayCounter): The day count turning dates into times.

        Raises:
            ItofinError: On whatever the core rejects about the nodes.
        """
        ...

class PiecewiseYieldCurve(YieldTermStructure):
    """A yield curve bootstrapped from a strip of rate helpers, one node per maturity.

    Every helper is solved so it reprices its own market quote off the curve.
    This string-dispatch alias covers the Discount convention; the other
    bootstrap conventions are reached through the named Piecewise* classes,
    which also expose node introspection.

    The bootstrap is lazy: construction only rejects an empty helper list, and
    the solver runs on the first query, re-running after a helper-quote or
    evaluation-date change. A bootstrap failure therefore surfaces from the
    query methods, not from the constructor.
    """

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
        interpolation: str = "LogLinear",
    ) -> None:
        """Build the curve over helpers with a fixed reference date.

        Args:
            reference_date (Date): The curve's reference date, typically the
                settlement date the caller computed.
            helpers (list[RateHelper]): The bootstrap instruments; any
                RateHelper subclass is accepted.
            day_counter (DayCounter): The day count turning dates into times.
            interpolation (str): "LogLinear" or "Linear". "Cubic" is refused: a
                global interpolator cannot converge under the single-pass
                bootstrap, and it is available on ZeroCurve and DiscountCurve
                instead.

        Raises:
            ItofinError: On an empty helper list and on an unknown or refused
                interpolation name.
        """
        ...

class PiecewiseLogLinearDiscount(YieldTermStructure):
    """A curve bootstrapped in discount-factor space with log-linear interpolation.

    The verbatim QuantLib-SWIG name for the blessed (Discount, LogLinear)
    combination. Unlike the PiecewiseYieldCurve alias, the named class retains
    the concrete curve so it can expose the node introspection the erased
    handle discards. data() are discount factors, so data()[0] is the reference
    node's 1.0.
    """

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None:
        """Build the curve over helpers with a fixed reference date.

        Args:
            reference_date (Date): The curve's reference date.
            helpers (list[RateHelper]): The bootstrap instruments; any
                RateHelper subclass is accepted.
            day_counter (DayCounter): The day count turning dates into times.

        Raises:
            ItofinError: On an empty helper list.
        """
        ...
    def dates(self) -> list[Date]:
        """Return the bootstrapped node dates, triggering the lazy bootstrap.

        Returns:
            list[Date]: One date per helper maturity, plus the reference node.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def data(self) -> list[float]:
        """Return the bootstrapped node values, triggering the lazy bootstrap.

        Returns:
            list[float]: The discount factors, the first being 1.0.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...

class PiecewiseLinearZero(YieldTermStructure):
    """A curve bootstrapped in zero-rate space with linear interpolation.

    The verbatim QuantLib-SWIG name for the blessed (ZeroYield, Linear)
    combination. data() are continuously-compounded zero rates, so data()[0]
    mirrors the first solved pillar's rate rather than a 1.0 discount.
    """

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None:
        """Build the curve over helpers with a fixed reference date.

        Args:
            reference_date (Date): The curve's reference date.
            helpers (list[RateHelper]): The bootstrap instruments.
            day_counter (DayCounter): The day count turning dates into times.

        Raises:
            ItofinError: On an empty helper list.
        """
        ...
    def dates(self) -> list[Date]:
        """Return the bootstrapped node dates, triggering the lazy bootstrap.

        Returns:
            list[Date]: One date per helper maturity, plus the reference node.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def data(self) -> list[float]:
        """Return the bootstrapped node values, triggering the lazy bootstrap.

        Returns:
            list[float]: The zero rates at the nodes.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...

class PiecewiseLinearForward(YieldTermStructure):
    """A curve bootstrapped in instantaneous forward-rate space, interpolating linearly.

    The verbatim QuantLib-SWIG name for the blessed (ForwardRate, Linear)
    combination. data() are instantaneous forward rates.
    """

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None:
        """Build the curve over helpers with a fixed reference date.

        Args:
            reference_date (Date): The curve's reference date.
            helpers (list[RateHelper]): The bootstrap instruments.
            day_counter (DayCounter): The day count turning dates into times.

        Raises:
            ItofinError: On an empty helper list.
        """
        ...
    def dates(self) -> list[Date]:
        """Return the bootstrapped node dates, triggering the lazy bootstrap.

        Returns:
            list[Date]: One date per helper maturity, plus the reference node.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def data(self) -> list[float]:
        """Return the bootstrapped node values, triggering the lazy bootstrap.

        Returns:
            list[float]: The instantaneous forward rates at the nodes.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...

class PiecewiseFlatForward(YieldTermStructure):
    """A curve bootstrapped in forward-rate space, interpolating backward-flat.

    The verbatim QuantLib-SWIG name for the blessed (ForwardRate, BackwardFlat)
    combination. Piecewise-constant instantaneous forwards make it numerically
    identical to PiecewiseLogLinearDiscount under every query; only data(),
    forward rates against discount factors, tells the two apart.
    """

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None:
        """Build the curve over helpers with a fixed reference date.

        Args:
            reference_date (Date): The curve's reference date.
            helpers (list[RateHelper]): The bootstrap instruments.
            day_counter (DayCounter): The day count turning dates into times.

        Raises:
            ItofinError: On an empty helper list.
        """
        ...
    def dates(self) -> list[Date]:
        """Return the bootstrapped node dates, triggering the lazy bootstrap.

        Returns:
            list[Date]: One date per helper maturity, plus the reference node.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def data(self) -> list[float]:
        """Return the bootstrapped node values, triggering the lazy bootstrap.

        Returns:
            list[float]: The instantaneous forward rates at the nodes.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...

class BlackConstantVol(BlackVolTermStructure):
    """A flat Black volatility, constant in strike and time.

    Unbounded in both time and strike, so queries never need extrapolation
    enabled.
    """

    def __init__(
        self,
        reference_date: Date,
        volatility: float,
        day_counter: DayCounter,
        calendar: Calendar | None = None,
    ) -> None:
        """Build the flat surface.

        Args:
            reference_date (Date): The date times are measured from.
            volatility (float): The single volatility answered everywhere.
            day_counter (DayCounter): The day count turning dates into times.
            calendar (Calendar | None): The surface's calendar, if any.
        """
        ...

class BlackVolTimeExtrapolation:
    """How a variance curve extrapolates past its last node.

    ``UseInterpolator`` is accepted at construction but raises ItofinError on
    any extrapolating query: the interpolation layer cannot be evaluated past
    its last node, and the core errors rather than silently substituting
    another rule."""

    FlatVolatility: BlackVolTimeExtrapolation
    UseInterpolator: BlackVolTimeExtrapolation
    LinearVariance: BlackVolTimeExtrapolation

class BlackVarianceCurve(BlackVolTermStructure):
    """A term structure of Black volatility with no strike dimension.

    Interpolates linearly on variance. Finite in time: the last date is the
    maximum, so queries past it require enable_extrapolation(), and
    time_extrapolation picks the rule applied there. The interpolation itself
    stays linear; only the extrapolation axis is exposed.
    """

    def __init__(
        self,
        reference_date: Date,
        dates: list[Date],
        black_vol_curve: list[float],
        day_counter: DayCounter,
        force_monotone_variance: bool,
        time_extrapolation: BlackVolTimeExtrapolation = ...,
    ) -> None:
        """Build the variance curve over its (date, volatility) nodes.

        Args:
            reference_date (Date): The date times are measured from.
            dates (list[Date]): The node dates.
            black_vol_curve (list[float]): The Black volatility at each node.
            day_counter (DayCounter): The day count turning dates into times.
            force_monotone_variance (bool): Whether to require the implied
                variance to increase across the nodes.
            time_extrapolation (BlackVolTimeExtrapolation): The rule applied
                past the last node; defaults to FlatVolatility, the C++
                default. Selecting UseInterpolator constructs fine and answers
                in-range queries, then errors on an extrapolating one.

        Raises:
            ItofinError: On whatever the core rejects about the nodes, a
                non-monotone variance under force_monotone_variance included.
        """
        ...

class BlackVarianceSurface(BlackVolTermStructure):
    """A Black volatility surface in strike and expiry, interpolating bilinearly.

    Finite in both time and strike, so out-of-grid queries require
    enable_extrapolation().
    """

    def __init__(
        self,
        reference_date: Date,
        dates: list[Date],
        strikes: list[float],
        black_vol_matrix: list[list[float]],
        day_counter: DayCounter,
        calendar: Calendar | None = None,
    ) -> None:
        """Build the surface over its strike-by-expiry grid.

        Args:
            reference_date (Date): The date times are measured from.
            dates (list[Date]): The expiry grid, one per matrix column.
            strikes (list[float]): The strike grid, one per matrix row.
            black_vol_matrix (list[list[float]]): The volatilities, one row per
                strike and one column per date.
            day_counter (DayCounter): The day count turning dates into times.
            calendar (Calendar | None): The surface's calendar, if any.

        Raises:
            ItofinError: On an empty or ragged matrix, and on whatever the core
                rejects about the grid dimensions.
        """
        ...

class RateHelper:
    """Shared base for every bootstrap helper: implied/market quotes and dates.

    A rate helper wraps a market quote plus the schedule of a single
    instrument; a piecewise curve is bootstrapped so every helper reprices its
    own quote. Concrete helpers subclass this and supply only their
    constructor.
    """

    def implied_quote(self) -> float:
        """Return the quote implied by the curve the helper is linked to.

        Returns:
            float: The curve-implied quote.

        Raises:
            ItofinError: With no curve set, the pre-bootstrap state, there
                being nothing to imply from.
        """
        ...
    def quote_error(self) -> float:
        """Return the bootstrap root: market quote minus implied quote.

        Returns:
            float: The residual the solver drives to zero.

        Raises:
            ItofinError: On the same condition implied_quote reports.
        """
        ...
    def quote_value(self) -> float:
        """Return the current value of the market quote the helper fits.

        Reads back through the retained quote handle, so a set_value on the
        SimpleQuote passed to the constructor is observed here: the same-object
        wiring the laziness contract relies on.

        Returns:
            float: The market quote's current value.
        """
        ...
    def maturity_date(self) -> Date:
        """Return the instrument's maturity date.

        Returns:
            Date: The maturity.
        """
        ...
    def pillar_date(self) -> Date:
        """Return the date the curve node this helper sets sits at.

        Returns:
            Date: The pillar date.
        """
        ...
    def earliest_date(self) -> Date:
        """Return the earliest date the helper needs curve data at.

        Returns:
            Date: The earliest relevant date.
        """
        ...
    def latest_date(self) -> Date:
        """Return the latest date the helper needs curve data at.

        Returns:
            Date: The latest date, equal to the pillar date.
        """
        ...
    def latest_relevant_date(self) -> Date:
        """Return the latest date whose data the helper is relevant for.

        Returns:
            Date: The latest relevant date.
        """
        ...

class DepositRateHelper(RateHelper):
    """A helper fitting a deposit rate."""

    def __init__(self, quote: SimpleQuote, index: IborIndex) -> None:
        """Build the helper over a live quote.

        Args:
            quote (SimpleQuote): The deposit rate; the caller keeps it, and
                mutating it later invalidates the bootstrap.
            index (IborIndex): The index supplying the deposit's schedule.
        """
        ...
    @staticmethod
    def from_rate(rate: float, index: IborIndex) -> DepositRateHelper:
        """Build the helper over a fixed rate.

        Args:
            rate (float): The deposit rate, wrapped in an internal quote the
                caller cannot later mutate.
            index (IborIndex): The index supplying the deposit's schedule.

        Returns:
            DepositRateHelper: The helper fitting that rate.
        """
        ...

class SwapRateHelper(RateHelper):
    """A helper fitting a par swap rate (spot-starting, no spread).

    The spot-starting form the curve-consistency oracle builds: no spread, no
    forward start, no exogenous discounting curve, and the default pillar.
    """

    def __init__(
        self,
        quote: SimpleQuote,
        tenor: Period,
        calendar: Calendar,
        fixed_frequency: Frequency,
        fixed_convention: BusinessDayConvention,
        fixed_day_count: DayCounter,
        ibor_index: IborIndex,
    ) -> None:
        """Build the helper over the schedule of a spot-starting swap.

        Args:
            quote (SimpleQuote): The par swap rate the helper fits.
            tenor (Period): The length of the swap.
            calendar (Calendar): The calendar the schedule rolls on.
            fixed_frequency (Frequency): The fixed leg's payment frequency.
            fixed_convention (BusinessDayConvention): The fixed leg's roll.
            fixed_day_count (DayCounter): The fixed leg's day count.
            ibor_index (IborIndex): The index the floating leg fixes off.
        """
        ...

class FuturesType:
    """The date convention an interest-rate future settles on.

    Imm and Custom are fully usable from Python. Asx validates and prices against
    an explicitly supplied ASX start date, but the ASX date navigators (the
    analogues of itofin.time.is_imm_date / next_imm_date) are deferred, so there
    is no helper to derive the next ASX date from Python yet."""

    Imm: FuturesType
    Asx: FuturesType
    Custom: FuturesType

class FuturesRateHelper(RateHelper):
    """A helper fitting an exchange-traded interest-rate future's quoted price.

    Unlike the deposit and swap helpers the window is absolute: it is computed
    once from the supplied dates and never rebuilt on an evaluation-date
    change. The convexity adjustment is usually absent; pass conv_adj=None to
    leave it empty, which reports a zero adjustment.
    """

    def __init__(
        self,
        price: SimpleQuote,
        ibor_start_date: Date,
        length_in_months: int,
        calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        day_counter: DayCounter,
        conv_adj: SimpleQuote | None,
        futures_type: FuturesType,
    ) -> None:
        """Build the helper over a length-in-months window off the start date.

        Args:
            price (SimpleQuote): The future's quoted price.
            ibor_start_date (Date): The window's start.
            length_in_months (int): The months the start is advanced by to
                reach maturity.
            calendar (Calendar): The calendar the maturity rolls on.
            convention (BusinessDayConvention): The roll applied to the
                maturity.
            end_of_month (bool): Whether the maturity roll keeps to month ends.
            day_counter (DayCounter): The day count the year fraction uses.
            conv_adj (SimpleQuote | None): The convexity quote, or None for an
                empty, zero adjustment.
            futures_type (FuturesType): The date convention the future settles
                on.

        Raises:
            ItofinError: If an Imm or Asx start is not a valid date of that
                convention.
        """
        ...
    @staticmethod
    def from_end_date(
        price: SimpleQuote,
        ibor_start_date: Date,
        ibor_end_date: Date | None,
        day_counter: DayCounter,
        conv_adj: SimpleQuote | None,
        futures_type: FuturesType,
    ) -> FuturesRateHelper:
        """Build the helper over an explicit window.

        Args:
            price (SimpleQuote): The future's quoted price.
            ibor_start_date (Date): The window's start.
            ibor_end_date (Date | None): The window's end, which must be past
                the start; None puts the maturity three IMM/ASX periods past
                the start.
            day_counter (DayCounter): The day count the year fraction uses.
            conv_adj (SimpleQuote | None): The convexity quote, or None for an
                empty, zero adjustment.
            futures_type (FuturesType): The date convention the future settles
                on.

        Returns:
            FuturesRateHelper: The helper over that window.

        Raises:
            ItofinError: On a Custom helper with no end date - a divergence
                from C++, which builds a null-maturity helper instead - and on
                a start that is not a valid date of the chosen convention.
        """
        ...
    @staticmethod
    def from_index(
        price: SimpleQuote,
        ibor_start_date: Date,
        index: IborIndex,
        conv_adj: SimpleQuote | None,
        futures_type: FuturesType,
    ) -> FuturesRateHelper:
        """Build the helper with a window following the index's conventions.

        The maturity is the start advanced by the index tenor on the index's
        fixing calendar, and the year fraction uses the index day counter.

        Args:
            price (SimpleQuote): The future's quoted price.
            ibor_start_date (Date): The window's start.
            index (IborIndex): The index supplying the conventions.
            conv_adj (SimpleQuote | None): The convexity quote, or None for an
                empty, zero adjustment.
            futures_type (FuturesType): The date convention the future settles
                on.

        Returns:
            FuturesRateHelper: The helper over that window.

        Raises:
            ItofinError: If the start is not a valid date of the chosen
                convention.
        """
        ...
    def convexity_adjustment(self) -> float:
        """Return the convexity adjustment applied to the forward.

        Returns:
            float: The convexity quote's value, or zero when none was supplied.
        """
        ...

class Pillar:
    """The date the curve node a helper fits sits at.

    MaturityDate and LastRelevantDate (the default) are the two schedule-derived
    choices. Pillar.CustomDate is deferred in the core (#343), so its omission
    here is deliberate, not an oversight."""

    MaturityDate: Pillar
    LastRelevantDate: Pillar

class FraRateHelper(RateHelper):
    """A helper fitting a forward-rate-agreement rate over the window starting
    period_to_start after spot and spanning the index tenor. use_indexed_coupon
    (default True) selects the indexed implied-quote mode; False is the par simple
    forward. from_dates fixes the window at construction (it does not shift on an
    evaluation-date change)."""

    def __init__(
        self,
        quote: SimpleQuote,
        period_to_start: Period,
        index: IborIndex,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> None:
        """Build the helper over the window period_to_start past spot.

        Args:
            quote (SimpleQuote): The FRA rate; the caller keeps it, so a later
                set_value re-drives the bootstrap.
            period_to_start (Period): How long after spot the window starts.
            index (IborIndex): The index whose tenor the window spans.
            use_indexed_coupon (bool): True selects the indexed implied-quote
                mode, the index fixing forecast off the curve; False is the par
                simple forward over the raw window.
            pillar (Pillar): The date the curve node sits at; defaults to
                LastRelevantDate.
        """
        ...
    @staticmethod
    def from_rate(
        rate: float,
        period_to_start: Period,
        index: IborIndex,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> FraRateHelper:
        """Build the helper over a fixed rate.

        Args:
            rate (float): The FRA rate, wrapped in an internal quote the caller
                cannot later mutate.
            period_to_start (Period): How long after spot the window starts.
            index (IborIndex): The index whose tenor the window spans.
            use_indexed_coupon (bool): The implied-quote mode; see __init__.
            pillar (Pillar): The date the curve node sits at.

        Returns:
            FraRateHelper: The helper fitting that rate.
        """
        ...
    @staticmethod
    def from_months(
        quote: SimpleQuote,
        months_to_start: int,
        index: IborIndex,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> FraRateHelper:
        """Build the helper with a start given in months after spot.

        Args:
            quote (SimpleQuote): The FRA rate the helper fits.
            months_to_start (int): How many months after spot the window
                starts.
            index (IborIndex): The index whose tenor the window spans.
            use_indexed_coupon (bool): The implied-quote mode; see __init__.
            pillar (Pillar): The date the curve node sits at.

        Returns:
            FraRateHelper: The helper over that window.
        """
        ...
    @staticmethod
    def from_dates(
        quote: SimpleQuote,
        start_date: Date,
        end_date: Date,
        index: IborIndex,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> FraRateHelper:
        """Build the helper over an explicit window.

        The schedule is fixed at construction and does not shift when the
        evaluation date changes.

        Args:
            quote (SimpleQuote): The FRA rate the helper fits.
            start_date (Date): The window's start.
            end_date (Date): The window's end.
            index (IborIndex): The index the forward is read off.
            use_indexed_coupon (bool): The implied-quote mode; see __init__.
            pillar (Pillar): The date the curve node sits at.

        Returns:
            FraRateHelper: The helper over that window.
        """
        ...

class RateAveraging:
    """How an overnight coupon combines its daily fixings.

    Simple is the arithmetic average; Compound (daily compounding) is the coupon
    default the OIS conventions use."""

    Simple: RateAveraging
    Compound: RateAveraging

class OISRateHelper(RateHelper):
    """A helper fitting an overnight-indexed swap rate (spot-starting, floating
    off an overnight index).

    The required knobs come first so settings can sit among them; the four
    optional knobs trail with defaults. discounting_curve=None discounts off the
    bootstrapping curve; overnight_spread=None is an empty (zero) spread. The
    deferred core knobs past averaging_method (telescopic value dates, lookback,
    lockout, observation shift, custom pillar, per-leg calendars) take benign
    defaults."""

    def __init__(
        self,
        settlement_days: int,
        tenor: Period,
        quote: SimpleQuote,
        overnight_index: OvernightIndex,
        payment_lag: int,
        payment_convention: BusinessDayConvention,
        payment_frequency: Frequency,
        forward_start: Period,
        settings: Settings,
        discounting_curve: YieldTermStructure | None = None,
        overnight_spread: SimpleQuote | None = None,
        pillar: Pillar = ...,
        averaging_method: RateAveraging = ...,
    ) -> None:
        """Build the helper over the schedule of a spot-starting OIS.

        Args:
            settlement_days (int): The days after the evaluation date the swap
                starts.
            tenor (Period): The length of the swap.
            quote (SimpleQuote): The OIS rate; the caller keeps it, so a later
                set_value re-drives the bootstrap.
            overnight_index (OvernightIndex): The index the floating leg
                compounds.
            payment_lag (int): The days between accrual end and payment.
            payment_convention (BusinessDayConvention): The roll applied to the
                payment dates.
            payment_frequency (Frequency): The payment frequency.
            forward_start (Period): How long after spot the swap starts.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
            discounting_curve (YieldTermStructure | None): The curve the flows
                discount on; None discounts off the bootstrapping curve.
            overnight_spread (SimpleQuote | None): The spread over the index;
                None leaves it empty, so zero. The caller keeps it, and
                mutating it re-drives the bootstrap.
            pillar (Pillar): The date the curve node sits at; defaults to
                LastRelevantDate.
            averaging_method (RateAveraging): How the daily fixings combine;
                defaults to Compound.
        """
        ...

class SwaptionVolatilityStructure:
    """Shared base for every swaption volatility surface: volatility, Black
    variance and lognormal shift, addressed by option and swap tenor."""

    def volatility(
        self,
        option_tenor: Period,
        swap_tenor: Period,
        strike: float,
        extrapolate: bool = False,
    ) -> float:
        """Return the volatility for an option tenor, swap tenor and strike.

        Args:
            option_tenor (Period): The option's tenor, resolved against the
                surface's reference date and calendar.
            swap_tenor (Period): The underlying swap's tenor.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The volatility, in whichever type the surface quotes.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def black_variance(
        self,
        option_tenor: Period,
        swap_tenor: Period,
        strike: float,
        extrapolate: bool = False,
    ) -> float:
        """Return the Black variance, the squared volatility times option time.

        Args:
            option_tenor (Period): The option's tenor.
            swap_tenor (Period): The underlying swap's tenor.
            strike (float): The strike the variance is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The Black variance.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def shift(
        self, option_date: Date, swap_length: float, extrapolate: bool = False
    ) -> float:
        """Return the lognormal shift, in the date form.

        Taken in the date form because the core trait has no tenor overload for
        the shift, unlike the volatility and variance queries above.

        Args:
            option_date (Date): The option date the shift is read at.
            swap_length (float): The underlying swap's length, in years.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The lognormal shift.

        Raises:
            ItofinError: On a normal-volatility surface, where a shift has no
                meaning, and on an out-of-grid query without extrapolation.
        """
        ...

class VolatilityType:
    """Whether a surface quotes shifted-lognormal (Black) or normal (Bachelier)
    volatilities. A mismatch with the engine's formula surfaces at pricing time."""

    ShiftedLognormal: VolatilityType
    Normal: VolatilityType

class ConstantSwaptionVolatility(SwaptionVolatilityStructure):
    """A single volatility with no option-time, swap-length or strike dependence.

    Both constructors pin the reference date, so every query's option time runs
    from reference_date rather than the evaluation date. The moving (floating
    reference date) forms are not exposed."""

    def __init__(
        self,
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: float,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shift: float = 0.0,
    ) -> None:
        """Build the surface at a fixed volatility.

        Args:
            reference_date (Date): The date every query's option time runs
                from.
            calendar (Calendar): The calendar option tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            volatility (float): The single volatility answered everywhere,
                wrapped in an internal quote the caller cannot later mutate.
            day_counter (DayCounter): The day count option times are measured
                in.
            volatility_type (VolatilityType): Whether the quote is
                shifted-lognormal or normal.
            shift (float): The lognormal shift.
        """
        ...
    @staticmethod
    def with_quote(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: SimpleQuote,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shift: float = 0.0,
    ) -> ConstantSwaptionVolatility:
        """Build the surface reading its volatility from a live quote.

        Args:
            reference_date (Date): The date every query's option time runs
                from.
            calendar (Calendar): The calendar option tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            volatility (SimpleQuote): The volatility; a later set_value
                notifies the surface's observers.
            day_counter (DayCounter): The day count option times are measured
                in.
            volatility_type (VolatilityType): Whether the quote is
                shifted-lognormal or normal.
            shift (float): The lognormal shift.

        Returns:
            ConstantSwaptionVolatility: The surface over that quote.
        """
        ...

class SwaptionVolatilityMatrix(SwaptionVolatilityStructure):
    """An at-the-money volatility grid, bilinear over an option-tenor x
    swap-tenor lattice.

    Every grid is a row per option tenor and a column per swap tenor; shifts,
    when given, must match that shape, and None means all-zero shifts. The grid
    is at the money, so a query's strike is range-checked and then ignored.
    flat_extrapolation clamps a query past the grid to the nearest edge or
    corner vol instead of extending the boundary surface."""

    def __init__(
        self,
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: list[Period],
        swap_tenors: list[Period],
        volatilities: list[list[float]],
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shifts: list[list[float]] | None = None,
        flat_extrapolation: bool = False,
    ) -> None:
        """Build the grid on a pinned reference date over fixed volatilities.

        Every query's option time runs from reference_date rather than from the
        evaluation date.

        Args:
            reference_date (Date): The date option times run from.
            calendar (Calendar): The calendar option tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            option_tenors (list[Period]): The option axis, one per grid row.
            swap_tenors (list[Period]): The swap axis, one per grid column.
            volatilities (list[list[float]]): The at-the-money volatilities,
                one row per option tenor.
            day_counter (DayCounter): The day count option times are measured
                in.
            volatility_type (VolatilityType): Whether the grid is
                shifted-lognormal or normal.
            shifts (list[list[float]] | None): The lognormal shifts in the same
                shape as volatilities; None means all-zero shifts.
            flat_extrapolation (bool): Whether a query past the grid clamps to
                the nearest edge or corner vol instead of extending the
                boundary surface.

        Raises:
            ItofinError: On an empty or ragged grid, a shifts grid that does
                not match the volatilities shape, and on whatever the core
                rejects about the axes.
        """
        ...
    @staticmethod
    def moving(
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: list[Period],
        swap_tenors: list[Period],
        volatilities: list[list[SimpleQuote]],
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        settings: Settings,
        shifts: list[list[float]] | None = None,
        flat_extrapolation: bool = False,
    ) -> SwaptionVolatilityMatrix:
        """Build a grid whose reference date floats off the evaluation date.

        The reference date sits at zero settlement days from the evaluation
        date, and each node is read from the caller's quote.

        Args:
            calendar (Calendar): The calendar option tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            option_tenors (list[Period]): The option axis, one per grid row.
            swap_tenors (list[Period]): The swap axis, one per grid column.
            volatilities (list[list[SimpleQuote]]): The at-the-money volatility
                quotes; a later set_value on any of them rebuilds the
                interpolation and notifies the grid's observers.
            day_counter (DayCounter): The day count option times are measured
                in.
            volatility_type (VolatilityType): Whether the grid is
                shifted-lognormal or normal.
            settings (Settings): The explicit settings supplying the evaluation
                date the reference date floats off.
            shifts (list[list[float]] | None): The lognormal shifts in the same
                shape as volatilities; None means all-zero shifts.
            flat_extrapolation (bool): Whether a query past the grid clamps to
                the nearest edge or corner vol.

        Returns:
            SwaptionVolatilityMatrix: The moving grid.

        Raises:
            ItofinError: On an empty or ragged grid, a mismatched shifts shape,
                and on whatever the core rejects about the axes.
        """
        ...

class InterpolatedSwaptionVolatilityCube(SwaptionVolatilityStructure):
    """A smile cube adding bilinearly-interpolated volatility spreads to an
    at-the-money surface.

    The inherited volatility query now takes a real strike: the cube reads the
    at-the-money forward off its base swap indexes, the at-the-money volatility
    off atm_vol, and adds the spread interpolated at strike - atm_strike.

    vol_spreads is row-major over the (option tenor, swap tenor) nodes: row
    i * len(swap_tenors) + j is the smile at (option_tenors[i], swap_tenors[j]),
    holding one quote per entry of strike_spreads. A later set_value on any of
    those quotes rebuilds the per-strike interpolators."""

    def __init__(
        self,
        atm_vol: SwaptionVolatilityStructure,
        option_tenors: list[Period],
        swap_tenors: list[Period],
        strike_spreads: list[float],
        vol_spreads: list[list[SimpleQuote]],
        swap_index_base: SwapIndex,
        short_swap_index_base: SwapIndex,
        settings: Settings,
        vega_weighted_smile_fit: bool = False,
    ) -> None:
        """Build the cube over an at-the-money surface and its vol spreads.

        Args:
            atm_vol (SwaptionVolatilityStructure): The at-the-money surface the
                spreads are added to.
            option_tenors (list[Period]): The option axis of the node grid.
            swap_tenors (list[Period]): The swap axis of the node grid.
            strike_spreads (list[float]): The moneyness offsets each smile is
                quoted at.
            vol_spreads (list[list[SimpleQuote]]): The spread quotes, row-major
                over the nodes with one quote per strike spread; a later
                set_value rebuilds the per-strike interpolators.
            swap_index_base (SwapIndex): The long base swap index.
            short_swap_index_base (SwapIndex): The short base swap index, whose
                tenor must not exceed the long one's.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
            vega_weighted_smile_fit (bool): Whether the smile fit is
                vega-weighted.

        Raises:
            ItofinError: On an empty or ragged vol_spreads grid, on a row count
                that is not one per node or a row length that is not one per
                strike spread, and on whatever the core rejects.
        """
        ...
    def atm_strike_from_tenor(self, option_tenor: Period, swap_tenor: Period) -> float:
        """Return the at-the-money strike for an option tenor and swap tenor.

        The fixing of whichever base swap index the swap tenor selects, off the
        option date the tenor resolves to against the cube's reference date and
        calendar. It lives on the concrete cube rather than the inherited base
        because it belongs to the cube framework, not the volatility structure.

        Args:
            option_tenor (Period): The option's tenor.
            swap_tenor (Period): The underlying swap's tenor, which selects the
                base index.

        Returns:
            float: The at-the-money strike a query is centred on.

        Raises:
            ItofinError: On whatever the selected index's fixing reports, an
                unset evaluation date or an unlinked forwarding curve included.
        """
        ...

class SabrSmileSection:
    """One option expiry's volatility smile, read off the closed-form Hagan SABR
    formula at fixed parameters.

    There is no calibration here: the four parameters are inputs. A fitted smile
    is what SabrSwaptionVolatilityCube serves; this class is for querying a smile
    whose parameters are already known."""

    def __init__(
        self,
        exercise_time: float,
        forward: float,
        alpha: float,
        beta: float,
        nu: float,
        rho: float,
        shift: float = 0.0,
        volatility_type: VolatilityType = ...,
    ) -> None:
        """Build the smile at a given exercise time and forward.

        Only the exercise-time form is wrapped; the date-anchored one differs
        from it only in computing that time from a reference date and a day
        counter, which a caller can do with DayCounter.year_fraction.

        Args:
            exercise_time (float): The option's exercise time, in years.
            forward (float): The forward the smile is centred on.
            alpha (float): The SABR alpha, which must be positive.
            beta (float): The SABR beta, which must lie in [0, 1].
            nu (float): The SABR nu, which must be non-negative.
            rho (float): The SABR rho, whose square must be below 1.
            shift (float): The lognormal shift; a non-zero shift is deferred
                and refused.
            volatility_type (VolatilityType): The quoting convention; Normal is
                deferred and refused.

        Raises:
            ItofinError: On a non-zero shift or a Normal volatility_type, both
                deferred; on a non-positive shifted forward; and on SABR
                parameters outside their admissible ranges.
        """
        ...
    def volatility(self, strike: float) -> float:
        """Return the volatility at strike.

        Args:
            strike (float): The strike; strikes below the shifted domain floor
                are clamped to it rather than rejected, as the core does.

        Returns:
            float: The Hagan SABR volatility.

        Raises:
            ItofinError: On whatever the closed-form evaluation rejects.
        """
        ...
    def variance(self, strike: float) -> float:
        """Return the Black variance at strike.

        Args:
            strike (float): The strike the variance is read at.

        Returns:
            float: The squared volatility times the exercise time.

        Raises:
            ItofinError: On whatever the closed-form evaluation rejects.
        """
        ...
    @property
    def exercise_time(self) -> float:
        """The exercise time the smile was built for.

        Returns:
            float: The exercise time, in years.
        """
        ...
    @property
    def atm_level(self) -> float:
        """The at-the-money level.

        Returns:
            float: The forward the smile is centred on.
        """
        ...
    @property
    def alpha(self) -> float:
        """The SABR alpha parameter.

        Returns:
            float: The alpha the smile was built with.
        """
        ...
    @property
    def beta(self) -> float:
        """The SABR beta parameter.

        Returns:
            float: The beta the smile was built with.
        """
        ...
    @property
    def nu(self) -> float:
        """The SABR nu parameter.

        Returns:
            float: The nu the smile was built with.
        """
        ...
    @property
    def rho(self) -> float:
        """The SABR rho parameter.

        Returns:
            float: The rho the smile was built with.
        """
        ...

class SabrSwaptionVolatilityCube(SwaptionVolatilityStructure):
    """A smile cube whose every node is a SABR smile fitted to the at-the-money
    volatility plus the market vol spreads.

    The inherited volatility query takes a real strike and answers off the fitted
    smile rather than an interpolated spread. Construction is where the work
    happens: every node is calibrated by Levenberg-Marquardt, and with
    is_atm_calibrated a second dense pass re-anchors the fitted smiles on the
    at-the-money surface.

    vol_spreads and parameters_guess are both row-major over the (option tenor,
    swap tenor) nodes: row i * len(swap_tenors) + j is the node at
    (option_tenors[i], swap_tenors[j]). A vol_spreads row holds one quote per
    entry of strike_spreads; a parameters_guess row holds the four SABR starting
    values [alpha, beta, nu, rho]. is_parameter_fixed pins a parameter at its
    guess across every node, in that same order.

    The end criteria, maximum error tolerance, optimisation method and accepted
    error are left at the core's C++ defaults. Backward-flat interpolation
    (core #606) is not exposed, and the optimisation method is always
    Levenberg-Marquardt, since a trait object does not cross FFI. ZABR and the
    generic XABR cube are a separate core track (#597), and the section-
    recalibration API is unported in the core: re-fit by bumping the guess or
    vol-spread quotes."""

    def __init__(
        self,
        atm_vol: SwaptionVolatilityStructure,
        option_tenors: list[Period],
        swap_tenors: list[Period],
        strike_spreads: list[float],
        vol_spreads: list[list[SimpleQuote]],
        swap_index_base: SwapIndex,
        short_swap_index_base: SwapIndex,
        parameters_guess: list[list[SimpleQuote]],
        is_parameter_fixed: list[bool],
        is_atm_calibrated: bool,
        settings: Settings,
        vega_weighted_smile_fit: bool = False,
        use_max_error: bool = False,
        max_guesses: int = 50,
        cutoff_strike: float = 0.0001,
    ) -> None:
        """Build the cube, calibrating every node on construction.

        The end criteria, the maximum error tolerance, the optimisation method
        and the accepted error are left at the core's C++ defaults.

        Args:
            atm_vol (SwaptionVolatilityStructure): The at-the-money surface the
                fitted smiles are anchored on.
            option_tenors (list[Period]): The option axis of the node grid.
            swap_tenors (list[Period]): The swap axis of the node grid.
            strike_spreads (list[float]): The moneyness offsets each smile is
                quoted at.
            vol_spreads (list[list[SimpleQuote]]): The spread quotes, row-major
                over the nodes with one quote per strike spread.
            swap_index_base (SwapIndex): The long base swap index.
            short_swap_index_base (SwapIndex): The short base swap index.
            parameters_guess (list[list[SimpleQuote]]): The SABR starting
                values, row-major over the nodes, each row holding alpha, beta,
                nu and rho in that order.
            is_parameter_fixed (list[bool]): Which of the four parameters are
                pinned at their guess across every node, in that same order.
            is_atm_calibrated (bool): Whether a second dense pass re-anchors
                the fitted smiles on the at-the-money surface.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
            vega_weighted_smile_fit (bool): Whether the smile fit is
                vega-weighted.
            use_max_error (bool): Whether the fit is judged on the maximum
                error rather than the aggregate one.
            max_guesses (int): How many starting guesses a node may try.
            cutoff_strike (float): The strike floor the fit is evaluated above.

        Raises:
            ItofinError: On an empty or ragged vol_spreads or parameters_guess
                grid, on a row count that is not one per node, on an
                is_parameter_fixed list that is not four entries long, on a
                normal at-the-money surface, which needs the deferred normal
                SABR formula, and on a calibration failure.
        """
        ...
    def atm_strike_from_tenor(self, option_tenor: Period, swap_tenor: Period) -> float:
        """Return the at-the-money strike for an option tenor and swap tenor.

        The fixing of whichever base swap index the swap tenor selects, and the
        strike the fitted smile is centred on, so it is what a caller needs to
        place a query at a given moneyness.

        Args:
            option_tenor (Period): The option's tenor.
            swap_tenor (Period): The underlying swap's tenor, which selects the
                base index.

        Returns:
            float: The at-the-money strike.

        Raises:
            ItofinError: On whatever the selected index's fixing reports, an
                unset evaluation date or an unlinked forwarding curve included.
        """
        ...

class OptionletVolatilityStructure:
    """Shared base for every caplet/floorlet volatility surface: volatility,
    Black variance and the lognormal displacement.

    A single option axis, unlike the swaption surfaces: a query takes one option
    tenor (or date) and a strike."""

    def volatility(
        self, option_tenor: Period, strike: float, extrapolate: bool = False
    ) -> float:
        """Return the caplet volatility for an option tenor and strike.

        Args:
            option_tenor (Period): The option's tenor, resolved against the
                surface's reference date and calendar.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The caplet volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def volatility_date(
        self, option_date: Date, strike: float, extrapolate: bool = False
    ) -> float:
        """Return the caplet volatility for an option date and strike.

        The date form the optionlet stripper and the cap/floor engine use, both
        addressing the surface by a coupon's fixing date.

        Args:
            option_date (Date): The option date the volatility is read at.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The caplet volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def black_variance(
        self, option_tenor: Period, strike: float, extrapolate: bool = False
    ) -> float:
        """Return the Black variance, the squared volatility times option time.

        Args:
            option_tenor (Period): The option's tenor.
            strike (float): The strike the variance is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The Black variance.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def allows_extrapolation(self) -> bool:
        """Return whether the surface answers dates and times beyond its maximum.

        Returns:
            bool: True when extrapolation is enabled on the surface itself.
        """
        ...
    def enable_extrapolation(self) -> None:
        """Allow extrapolation past the maximum date and time.

        A stripped surface ends at its last optionlet fixing, so a cap whose
        own last caplet fixes there queries the boundary.
        """
        ...
    def disable_extrapolation(self) -> None:
        """Forbid extrapolation past the maximum date and time."""
        ...
    def displacement(self) -> float:
        """Return the lognormal shift applied to forwards and strikes.

        This is what BlackCapFloorEngine checks a caller-supplied displacement
        against, so it is the number to read before pinning one on the engine.

        Returns:
            float: The shift; zero for the unshifted lognormal and the normal
                model.
        """
        ...

class ConstantOptionletVolatility(OptionletVolatilityStructure):
    """A single caplet volatility with no option-time or strike dependence.

    Both constructors pin the reference date, so every query's option time runs
    from reference_date rather than the evaluation date. The moving (floating
    reference date) forms are not exposed; tracked as #627."""

    def __init__(
        self,
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: float,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        displacement: float = 0.0,
    ) -> None:
        """Build the surface at a fixed volatility.

        Args:
            reference_date (Date): The date every query's option time runs
                from.
            calendar (Calendar): The calendar option tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            volatility (float): The single volatility answered everywhere,
                wrapped in an internal quote the caller cannot later mutate.
            day_counter (DayCounter): The day count option times are measured
                in.
            volatility_type (VolatilityType): Whether the quote is
                shifted-lognormal or normal.
            displacement (float): The lognormal shift applied to forwards and
                strikes.
        """
        ...
    @staticmethod
    def with_quote(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        volatility: SimpleQuote,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        displacement: float = 0.0,
    ) -> ConstantOptionletVolatility:
        """Build the surface reading its volatility from a live quote.

        Args:
            reference_date (Date): The date every query's option time runs
                from.
            calendar (Calendar): The calendar option tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            volatility (SimpleQuote): The volatility; a later set_value
                notifies the surface's observers.
            day_counter (DayCounter): The day count option times are measured
                in.
            volatility_type (VolatilityType): Whether the quote is
                shifted-lognormal or normal.
            displacement (float): The lognormal shift applied to forwards and
                strikes.

        Returns:
            ConstantOptionletVolatility: The surface over that quote.
        """
        ...

class CapFloorTermVolSurface:
    """The market cap/floor TERM-volatility surface, bicubic over an option-tenor
    x strike grid.

    This is the flat volatility of a WHOLE cap by cap length, which is how the
    market quotes caps, not the volatility of the individual caplets it
    decomposes into: it is the optionlet stripper's input, and it is not an
    OptionletVolatilityStructure.

    volatilities is a row per option tenor and a column per strike; both axes
    must be strictly increasing.

    All four constructors are exposed. __init__ and with_quotes pin the
    reference date, so every query's option time runs from reference_date rather
    than the evaluation date. moving and moving_with_quotes float it
    settlement_days off the evaluation date, and are what the optionlet
    stripping pipeline runs on: StrippedOptionletAdapter reads its settlement
    days back off this surface, and a pinned-reference surface has none."""

    def __init__(
        self,
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: list[Period],
        strikes: list[float],
        volatilities: list[list[float]],
        day_counter: DayCounter,
    ) -> None:
        """Build the surface on a pinned reference date over fixed volatilities.

        Every query's option time runs from reference_date, not from the
        evaluation date, and no later mutation can reach the grid.

        Args:
            reference_date (Date): The date option times run from.
            calendar (Calendar): The calendar cap tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            option_tenors (list[Period]): The cap-length axis, one per grid
                row; strictly increasing.
            strikes (list[float]): The strike axis, one per grid column;
                strictly increasing.
            volatilities (list[list[float]]): The flat cap volatilities, one
                row per option tenor.
            day_counter (DayCounter): The day count option times are measured
                in.

        Raises:
            ItofinError: On an empty or ragged grid, on a grid whose shape does
                not match the tenors and strikes, and on a non-increasing tenor
                or strike axis.
        """
        ...
    @staticmethod
    def with_quotes(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: list[Period],
        strikes: list[float],
        volatilities: list[list[SimpleQuote]],
        day_counter: DayCounter,
    ) -> CapFloorTermVolSurface:
        """Build the pinned-reference surface over live quotes.

        Args:
            reference_date (Date): The date option times run from.
            calendar (Calendar): The calendar cap tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            option_tenors (list[Period]): The cap-length axis, strictly
                increasing.
            strikes (list[float]): The strike axis, strictly increasing.
            volatilities (list[list[SimpleQuote]]): The volatility quotes, one
                row per option tenor; a later set_value rebuilds the
                interpolation and notifies the surface's observers.
            day_counter (DayCounter): The day count option times are measured
                in.

        Returns:
            CapFloorTermVolSurface: The surface over those quotes.

        Raises:
            ItofinError: On the same conditions __init__ reports.
        """
        ...
    @staticmethod
    def moving(
        settlement_days: int,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: list[Period],
        strikes: list[float],
        volatilities: list[list[float]],
        day_counter: DayCounter,
        settings: Settings,
    ) -> CapFloorTermVolSurface:
        """Build a surface whose reference date floats off the evaluation date.

        This is the form the optionlet stripping pipeline needs: unlike the
        pinned-reference constructors, it carries the settlement days
        StrippedOptionletAdapter reads back off the stripper.

        Args:
            settlement_days (int): The business days the reference date sits
                past the evaluation date.
            calendar (Calendar): The calendar cap tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            option_tenors (list[Period]): The cap-length axis, strictly
                increasing.
            strikes (list[float]): The strike axis, strictly increasing.
            volatilities (list[list[float]]): The flat cap volatilities, one
                row per option tenor.
            day_counter (DayCounter): The day count option times are measured
                in.
            settings (Settings): The explicit settings supplying the evaluation
                date the reference date floats off.

        Returns:
            CapFloorTermVolSurface: The floating-reference surface.

        Raises:
            ItofinError: On the same conditions __init__ reports.
        """
        ...
    @staticmethod
    def moving_with_quotes(
        settlement_days: int,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: list[Period],
        strikes: list[float],
        volatilities: list[list[SimpleQuote]],
        day_counter: DayCounter,
        settings: Settings,
    ) -> CapFloorTermVolSurface:
        """Build the floating-reference surface over live quotes.

        Args:
            settlement_days (int): The business days the reference date sits
                past the evaluation date.
            calendar (Calendar): The calendar cap tenors resolve on.
            business_day_convention (BusinessDayConvention): The roll applied
                when resolving a tenor to a date.
            option_tenors (list[Period]): The cap-length axis, strictly
                increasing.
            strikes (list[float]): The strike axis, strictly increasing.
            volatilities (list[list[SimpleQuote]]): The volatility quotes, one
                row per option tenor.
            day_counter (DayCounter): The day count option times are measured
                in.
            settings (Settings): The explicit settings supplying the evaluation
                date the reference date floats off.

        Returns:
            CapFloorTermVolSurface: The floating-reference surface over those
                quotes.

        Raises:
            ItofinError: On the same conditions __init__ reports.
        """
        ...
    def volatility(
        self, option_tenor: Period, strike: float, extrapolate: bool = False
    ) -> float:
        """Return the flat cap volatility for a cap tenor and strike.

        The tenor form resolves against the surface's own calendar and
        business-day convention, so it is the one to reach for unless a date is
        already in hand.

        Args:
            option_tenor (Period): The cap's length.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The flat cap volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def volatility_date(
        self, end_date: Date, strike: float, extrapolate: bool = False
    ) -> float:
        """Return the flat cap volatility for a cap end date and strike.

        Args:
            end_date (Date): The cap's end date.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The flat cap volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...
    def volatility_time(
        self, length: float, strike: float, extrapolate: bool = False
    ) -> float:
        """Return the flat cap volatility for a cap end time and strike.

        Args:
            length (float): A year fraction off the reference date, in the
                surface's own day count.
            strike (float): The strike the volatility is read at.
            extrapolate (bool): Whether to answer outside the surface's grid.

        Returns:
            float: The flat cap volatility.

        Raises:
            ItofinError: If the query falls outside the grid and extrapolation
                is not allowed.
        """
        ...

class OptionletStripper1:
    """Bootstraps caplet volatilities out of a market cap/floor term-volatility
    surface.

    Not itself a volatility surface: it produces a grid of caplet volatilities
    that StrippedOptionletAdapter interpolates into one. Stripping is lazy and
    cached, and re-runs only when a surface quote or the index changes.

    term_vol_surface must come from CapFloorTermVolSurface.moving or
    moving_with_quotes; a pinned-reference surface carries no settlement days
    and fails the adapter. VolatilityType.Normal is deferred (#440/#577) and
    fails at the strip, not at construction."""

    def __init__(
        self,
        term_vol_surface: CapFloorTermVolSurface,
        ibor_index: IborIndex,
        volatility_type: VolatilityType,
        accuracy: float = 1e-6,
        max_iter: int = 100,
        displacement: float = 0.0,
        discount: YieldTermStructure | None = None,
        optionlet_frequency: Period | None = None,
    ) -> None:
        """Build the stripper over a term-volatility surface and an index.

        It prices a cap at each of its own lengths off term_vol_surface,
        differences consecutive prices into a single caplet price, and inverts
        that for the caplet's implied volatility.

        Args:
            term_vol_surface (CapFloorTermVolSurface): The market term
                volatilities; it must be one of the moving forms, a
                pinned-reference surface carrying no settlement days.
            ibor_index (IborIndex): The index the caplets fix off.
            volatility_type (VolatilityType): The quoting convention; Normal is
                deferred and fails at the strip, not here.
            accuracy (float): The tolerance of the implied-volatility solve.
            max_iter (int): The iteration cap of that solve.
            displacement (float): The lognormal shift applied to forwards and
                strikes.
            discount (YieldTermStructure | None): The curve the caps are priced
                on; None falls back to the index's own forwarding curve.
            optionlet_frequency (Period | None): The caplet step; None uses the
                index tenor.

        Raises:
            ItofinError: On whatever the core rejects about the surface, the
                index or the solve parameters.
        """
        ...
    def switch_strike(self) -> float:
        """Return the floating switch strike, the mean at-the-money caplet rate.

        It decides whether each strike is stripped out of caps or out of
        floors. The first call triggers the strip.

        Returns:
            float: The switch strike.

        Raises:
            ItofinError: On a stripping failure, which a Normal volatility_type
                always is.
        """
        ...
    def atm_optionlet_rates(self) -> list[float]:
        """Return the at-the-money forward rate of each caplet.

        Returns:
            list[float]: One rate per maturity.

        Raises:
            ItofinError: On a stripping failure.
        """
        ...

class StrippedOptionletAdapter(OptionletVolatilityStructure):
    """Serves a stripper's caplet volatility grid as an
    OptionletVolatilityStructure: linear in strike within each maturity, then
    linear across maturities.

    This closes the cap/floor volatility loop - a BlackCapFloorEngine on this
    surface reprices the caps the term volatilities were quoted on. The
    reference date floats off the evaluation date carried by settings, advanced
    by the term-volatility surface's settlement days. The surface ends at the
    last caplet fixing, so pricing a cap that reaches it wants
    enable_extrapolation()."""

    def __init__(self, stripper: OptionletStripper1, settings: Settings) -> None:
        """Build the interpolated surface over a stripper.

        It strips eagerly: the constructor reads the caplet strikes and fixing
        dates to snapshot its strike domain and maximum date.

        Args:
            stripper (OptionletStripper1): The stripper whose caplet grid is
                served.
            settings (Settings): The explicit settings supplying the evaluation
                date the reference date floats off.

        Raises:
            ItofinError: On a stripper whose term-volatility surface carries no
                settlement days, which is every pinned-reference surface, and
                on a stripping failure.
        """
        ...

class DefaultProbabilityTermStructure:
    """Shared base for every credit curve: survival and default probabilities,
    the default density and the hazard rate, each in a year-fraction and a date
    form."""

    def survival_probability(self, t: float, extrapolate: bool = False) -> float:
        """Return the survival probability from the reference date to year-fraction t.

        Args:
            t (float): The year fraction, in the curve's own day count.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The probability of surviving to t.

        Raises:
            ItofinError: If t is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def survival_probability_date(self, date: Date, extrapolate: bool = False) -> float:
        """Return the survival probability from the reference date to date.

        Args:
            date (Date): The date survived to.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The survival probability.

        Raises:
            ItofinError: If date is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def default_probability(self, t: float, extrapolate: bool = False) -> float:
        """Return the default probability from the reference date to year-fraction t.

        Args:
            t (float): The year fraction, in the curve's own day count.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The probability of defaulting by t.

        Raises:
            ItofinError: If t is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def default_probability_date(self, date: Date, extrapolate: bool = False) -> float:
        """Return the default probability from the reference date to date.

        Args:
            date (Date): The date defaulted by.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The default probability.

        Raises:
            ItofinError: If date is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def default_density(self, t: float, extrapolate: bool = False) -> float:
        """Return the default density at year-fraction t.

        Args:
            t (float): The year fraction, in the curve's own day count.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The default density.

        Raises:
            ItofinError: If t is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def default_density_date(self, date: Date, extrapolate: bool = False) -> float:
        """Return the default density at date.

        Args:
            date (Date): The date the density is read at.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The default density.

        Raises:
            ItofinError: If date is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def hazard_rate(self, t: float, extrapolate: bool = False) -> float:
        """Return the hazard rate at year-fraction t.

        Args:
            t (float): The year fraction, in the curve's own day count.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The hazard rate, at annual frequency and continuous
                compounding.

        Raises:
            ItofinError: If t is past the curve's range and extrapolation is
                not allowed.
        """
        ...
    def hazard_rate_date(self, date: Date, extrapolate: bool = False) -> float:
        """Return the hazard rate at date.

        Args:
            date (Date): The date the rate is read at.
            extrapolate (bool): Whether to answer past the curve's max date.

        Returns:
            float: The hazard rate, at annual frequency and continuous
                compounding.

        Raises:
            ItofinError: If date is past the curve's range and extrapolation is
                not allowed.
        """
        ...

class FlatHazardRate(DefaultProbabilityTermStructure):
    """A credit curve quoting one hazard rate for every maturity, whose survival
    probability is the closed form exp(-h t).

    The quote-backed forms retain the caller's SimpleQuote, so a later set_value
    moves the curve; the rate-backed forms wrap the value in a fresh, un-retained
    quote. The moving forms fix the reference date settlement_days business days
    past the evaluation date carried by settings."""

    def __init__(
        self, reference_date: Date, hazard_rate: SimpleQuote, day_counter: DayCounter
    ) -> None:
        """Build a curve reading its hazard rate live, on a pinned reference date.

        Args:
            reference_date (Date): The date times are measured from.
            hazard_rate (SimpleQuote): The hazard rate; the caller keeps it, so
                a later set_value moves the curve.
            day_counter (DayCounter): The day count turning dates into times.
        """
        ...
    @staticmethod
    def with_rate(
        reference_date: Date, rate: float, day_counter: DayCounter
    ) -> FlatHazardRate:
        """Build a curve at a fixed rate, on a pinned reference date.

        Args:
            reference_date (Date): The date times are measured from.
            rate (float): The hazard rate, wrapped in a fresh, un-retained
                quote.
            day_counter (DayCounter): The day count turning dates into times.

        Returns:
            FlatHazardRate: The curve at that rate.
        """
        ...
    @staticmethod
    def moving(
        settlement_days: int,
        calendar: Calendar,
        hazard_rate: SimpleQuote,
        day_counter: DayCounter,
        settings: Settings,
    ) -> FlatHazardRate:
        """Build a curve reading its hazard rate live, on a floating reference date.

        The reference date sits settlement_days business days past the
        evaluation date, so a query made before settings carries one raises
        rather than falling back to a system clock.

        Args:
            settlement_days (int): The business days the reference date sits
                past the evaluation date.
            calendar (Calendar): The calendar those days are counted on.
            hazard_rate (SimpleQuote): The hazard rate; the caller keeps it, so
                a later set_value moves the curve.
            day_counter (DayCounter): The day count turning dates into times.
            settings (Settings): The explicit settings supplying the evaluation
                date.

        Returns:
            FlatHazardRate: The moving curve.
        """
        ...
    @staticmethod
    def moving_with_rate(
        settlement_days: int,
        calendar: Calendar,
        rate: float,
        day_counter: DayCounter,
        settings: Settings,
    ) -> FlatHazardRate:
        """Build a curve at a fixed rate, on a floating reference date.

        As moving(), a query made before settings carries an evaluation date
        raises rather than falling back to a system clock.

        Args:
            settlement_days (int): The business days the reference date sits
                past the evaluation date.
            calendar (Calendar): The calendar those days are counted on.
            rate (float): The hazard rate, wrapped in a fresh, un-retained
                quote.
            day_counter (DayCounter): The day count turning dates into times.
            settings (Settings): The explicit settings supplying the evaluation
                date.

        Returns:
            FlatHazardRate: The moving curve at that rate.
        """
        ...

class InterpolatedHazardRateCurve(DefaultProbabilityTermStructure):
    """A credit curve built from (date, hazard-rate) nodes, interpolating
    backward-flat.

    The first date is the reference date. Backward-flat reads the right-hand
    node on every segment, so the hazard rate is a right-continuous step
    function and the survival probability is exp(-integral) over those steps.
    Finite in time: queries past the last node need extrapolate=True, which
    continues at the last node's rate."""

    def __init__(
        self,
        dates: list[Date],
        hazard_rates: list[float],
        day_counter: DayCounter,
    ) -> None:
        """Build the curve over its (date, hazard-rate) nodes.

        Backward-flat is pinned at the boundary: it is the only interpolator
        the credit side wires, so no interpolation argument is offered.

        Args:
            dates (list[Date]): The node dates, the first being the reference
                date.
            hazard_rates (list[float]): The hazard rate at each node.
            day_counter (DayCounter): The day count turning dates into times.

        Raises:
            ItofinError: On too few dates, a dates and hazard_rates count
                mismatch, a negative hazard rate, or unsorted dates.
        """
        ...
    def dates(self) -> list[Date]:
        """Return the node dates.

        Returns:
            list[Date]: The nodes, the first of which is the reference date.
        """
        ...
    def hazard_rates(self) -> list[float]:
        """Return the node hazard rates.

        Returns:
            list[float]: The rate at each node.
        """
        ...
    def nodes(self) -> list[tuple[Date, float]]:
        """Return the curve's nodes as pairs.

        Returns:
            list[tuple[Date, float]]: One (date, hazard rate) pair per node.
        """
        ...

class DefaultProbabilityHelper:
    """Shared base for every credit bootstrap helper.

    A credit helper fits a default-probability curve rather than a yield curve,
    so it is a separate hierarchy from RateHelper. It exposes the two dates the
    bootstrap places a curve node by.
    """

    def pillar_date(self) -> Date:
        """Return the date the curve node this helper sets sits at.

        Returns:
            Date: The pillar date.
        """
        ...
    def latest_date(self) -> Date:
        """Return the latest date the helper needs curve data at.

        Returns:
            Date: The latest date, equal to the pillar date.
        """
        ...

class SpreadCdsHelper(DefaultProbabilityHelper):
    """Bootstrap helper fitting a CDS quoted as a running spread.

    The helper rebuilds its schedule and its contract off the evaluation date
    held by settings, so it tracks that date rather than freezing a maturity at
    construction. It retains the caller's quote, so a later set_value re-drives
    the bootstrap, and it observes the discount curve.
    """

    def __init__(
        self,
        running_spread: SimpleQuote,
        tenor: Period,
        settlement_days: int,
        calendar: Calendar,
        frequency: Frequency,
        payment_convention: BusinessDayConvention,
        rule: DateGeneration,
        day_counter: DayCounter,
        recovery_rate: float,
        discount_curve: YieldTermStructure,
        settings: Settings,
    ) -> None:
        """Build the helper on the C++ default CDS terms.

        Args:
            running_spread (SimpleQuote): The quoted spread the helper fits.
            tenor (Period): The length of the contract.
            settlement_days (int): The days between the evaluation date and the
                contract's start.
            calendar (Calendar): The calendar the schedule rolls on.
            frequency (Frequency): The premium payment frequency.
            payment_convention (BusinessDayConvention): The roll applied to the
                payment dates.
            rule (DateGeneration): The schedule generation rule.
            day_counter (DayCounter): The day count the premium accrues on.
            recovery_rate (float): The recovery assumed on default.
            discount_curve (YieldTermStructure): The curve the flows discount
                on; the helper observes it.
            settings (Settings): The explicit settings supplying the evaluation
                date the schedule is rebuilt off.

        Raises:
            ItofinError: Under the three post-Big-Bang rules OldCDS, CDS and
                CDS2015, whose maturity is rolled by the CDS maturity rule: it
                refuses a tenor it cannot roll, or one it rolls to a contract
                that has already matured, rather than building a schedule that
                ends on the wrong date.
        """
        ...

class PiecewiseDefaultCurve(DefaultProbabilityTermStructure):
    """A credit curve bootstrapped from CDS helpers, solving one hazard-rate
    node per helper maturity (PiecewiseDefaultCurve<HazardRate, BackwardFlat>).

    Lazy: the bootstrap runs on the first read, so the helpers' Settings flags
    and evaluation date must be in place before that read, not merely before
    the constructor. A helper quote moving invalidates the cache."""

    def __init__(
        self,
        reference_date: Date,
        helpers: list[DefaultProbabilityHelper],
        day_counter: DayCounter,
    ) -> None:
        """Build the curve over helpers with a fixed reference date.

        Args:
            reference_date (Date): The curve's reference date.
            helpers (list[DefaultProbabilityHelper]): The bootstrap
                instruments; any subclass is accepted.
            day_counter (DayCounter): The day count turning dates into times.

        Raises:
            ItofinError: On an empty helper list.
        """
        ...
    def calculate(self) -> None:
        """Run the bootstrap if the cache is stale.

        Calling it explicitly makes a solver failure surface here rather than
        inside a later query.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def times(self) -> list[float]:
        """Return the node times, triggering the bootstrap.

        Returns:
            list[float]: The nodes in the curve's own day count.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def dates(self) -> list[Date]:
        """Return the node dates, triggering the bootstrap.

        Returns:
            list[Date]: The nodes, the first of which is the reference date.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def data(self) -> list[float]:
        """Return the solved node hazard rates, triggering the bootstrap.

        Returns:
            list[float]: The rate solved at each node.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...
    def nodes(self) -> list[tuple[Date, float]]:
        """Return the solved nodes as pairs, triggering the bootstrap.

        Returns:
            list[tuple[Date, float]]: One (date, hazard rate) pair per node.

        Raises:
            ItofinError: On a bootstrap failure.
        """
        ...

class MultiplicativePriceSeasonality:
    """The seasonal correction a price index carries, whose factors multiply
    the index level itself.

    The factors are given in whole multiples of the count the frequency
    dictates - twelve for Frequency.Monthly - and are reused as long as needed,
    so twelve of them are stationary and twenty-four repeat every two years.
    They are not applied raw: the factor at the queried date is normalized
    against the one at a reference date, which for a zero rate is the curve's
    own base date, so the correction is the identity there.

    Install it with ZeroInflationTermStructure.set_seasonality. Only the
    date-taking rate query folds the correction in; the year-fraction one
    cannot, a time not naming the date the factors are a function of."""

    def __init__(
        self,
        seasonality_base_date: Date,
        frequency: Frequency,
        seasonality_factors: list[float],
    ) -> None:
        """Raises ItofinError on a frequency outside semiannual-through-daily
        (Frequency.Annual among them), an empty factor set, or a factor count
        that is not a whole multiple of the frequency."""
        ...
    def seasonality_base_date(self) -> Date: ...
    def frequency(self) -> Frequency: ...
    def seasonality_factors(self) -> list[float]: ...
    def seasonality_factor(self, to: Date) -> float:
        """The raw factor covering `to`, before any normalization against a
        reference date - not the correction the curve applies. The offset is
        counted in whole factor periods from the seasonality base date and
        wraps modulo the factor count, in both directions."""
        ...

class ZeroInflationTermStructure:
    """Shared base for every zero-coupon inflation curve: the zero-coupon
    inflation rate in a year-fraction and a date form, the base date, the
    fixing frequency and the seasonality correction the curve carries.

    The two rate reads are not interchangeable. zero_rate_date snaps its date
    to the start of the inflation period containing it, because a fixing
    applies to a whole period; zero_rate takes a year-fraction already measured
    under the curve's own day counter and quantizes nothing. Only the first
    folds in any seasonality."""

    def zero_rate(self, t: float, extrapolate: bool = False) -> float: ...
    def zero_rate_date(self, date: Date, extrapolate: bool = False) -> float: ...
    def base_date(self) -> Date: ...
    def frequency(self) -> Frequency: ...
    def set_seasonality(
        self, seasonality: MultiplicativePriceSeasonality | None
    ) -> None:
        """Installs seasonality on the curve, replacing whatever it carried;
        None clears it. A bootstrapped curve is invalidated here, so the next
        read re-solves against the new correction.

        Raises ItofinError from the consistency gate, which a multi-year factor
        set fails (a documented core deferral, #807). The store happens before
        the gate runs, as C++'s does, so a rejected correction is left
        installed - clear it with None before reading the curve again."""
        ...
    def has_seasonality(self) -> bool: ...

class InterpolatedZeroInflationCurve(ZeroInflationTermStructure):
    """A zero-coupon inflation curve built from (date, zero-rate) nodes,
    interpolating linearly in zero-rate space.

    The first date is the base date rather than the reference date, which is
    passed separately and normally follows it; node times are measured from the
    reference date, so the first one is negative."""

    def __init__(
        self,
        reference_date: Date,
        dates: list[Date],
        rates: list[float],
        frequency: Frequency,
        day_counter: DayCounter,
    ) -> None:
        """Raises ItofinError on fewer than two dates, a dates/rates count
        mismatch, a rate at or below -100 % from the second node on, or
        unsorted dates."""
        ...
    def times(self) -> list[float]: ...
    def dates(self) -> list[Date]: ...
    def nodes(self) -> list[tuple[Date, float]]: ...

class ZeroInflationHelper:
    """Shared base for every zero-inflation bootstrap helper: the two dates the
    bootstrap places a curve node by.

    Concrete helpers such as ZeroCouponInflationSwapHelper subclass this and
    supply only their constructor."""

    def pillar_date(self) -> Date: ...
    def latest_date(self) -> Date: ...

class ZeroCouponInflationSwapHelper(ZeroInflationHelper):
    """The bootstrap helper fitting a zero-coupon inflation swap quoted as a
    rate.

    The helper prices a unit-notional, zero-strike swap of its own and reports
    that contract's fair rate; the bootstrap drives the quoted rate less that
    fair rate to zero. The swap starts at the evaluation date, so that date must
    be set before this constructor runs, not merely before the bootstrap.

    It prices through a copy of index linked to a handle of its own, so the
    caller's index need not be linked to any curve.

    pillar picks which of the two nodes an interpolated swap straddles the helper
    fits; a flat swap reads a single fixing and ignores it."""

    def __init__(
        self,
        quote: SimpleQuote,
        swap_obs_lag: Period,
        maturity: Date,
        calendar: Calendar,
        payment_convention: BusinessDayConvention,
        day_counter: DayCounter,
        index: ZeroInflationIndex,
        observation_interpolation: CpiInterpolationType,
        settings: Settings,
        pillar: Pillar = ...,
    ) -> None:
        """Raises ItofinError on an observation lag the index cannot observe
        through, and under CpiInterpolationType.Linear on one that leaves less
        than a whole index period over the index's availability lag."""
        ...
    def inflation_fixing_date(self) -> Date:
        """The maturity observation date on the helper's own swap: maturity less
        the observation lag, unsnapped. Not pillar_date, which is the first day
        of the period containing it."""
        ...

class PiecewiseZeroInflationCurve(ZeroInflationTermStructure):
    """A zero-coupon inflation curve bootstrapped from inflation helpers,
    solving one zero-rate node per helper fixing period.

    Node zero sits on base_date, not on reference_date, so times()[0] is
    negative - the one structural difference from every other piecewise curve.

    Lazy: the bootstrap runs on the first read, so the evaluation date must be
    in place before that read as well as before the helpers were built. A helper
    quote moving invalidates the cache."""

    def __init__(
        self,
        reference_date: Date,
        base_date: Date,
        frequency: Frequency,
        day_counter: DayCounter,
        helpers: list[ZeroInflationHelper],
    ) -> None:
        """Raises ItofinError on an empty helper list."""
        ...
    def calculate(self) -> None: ...
    def times(self) -> list[float]: ...
    def dates(self) -> list[Date]: ...
    def nodes(self) -> list[tuple[Date, float]]: ...

class YoYInflationTermStructure:
    """Shared base for every year-on-year inflation curve: the year-on-year
    rate in a year-fraction and a date form, the base date, the base rate, the
    fixing frequency and the seasonality correction the curve carries.

    The two rate reads are not interchangeable. yoy_rate_date snaps its date to
    the start of the inflation period containing it and is the only one that
    folds in any seasonality; yoy_rate takes a year-fraction already measured
    under the curve's own day counter and quantizes nothing. Neither is the
    year-on-year swap rate, which comes from the instrument.

    base_rate is answered here where the zero base defers it: a year-on-year
    curve carries the rate observed over the period ending on its base date."""

    def yoy_rate(self, t: float, extrapolate: bool = False) -> float: ...
    def yoy_rate_date(self, date: Date, extrapolate: bool = False) -> float: ...
    def base_date(self) -> Date: ...
    def base_rate(self) -> float: ...
    def frequency(self) -> Frequency: ...
    def set_seasonality(
        self, seasonality: MultiplicativePriceSeasonality | None
    ) -> None:
        """Installs seasonality on the curve, replacing whatever it carried;
        None clears it. Raises ItofinError from the consistency gate, leaving a
        rejected correction installed as C++ does."""
        ...
    def has_seasonality(self) -> bool: ...

class InterpolatedYoYInflationCurve(YoYInflationTermStructure):
    """A year-on-year inflation curve built from (date, year-on-year rate)
    nodes, interpolating linearly in rate space.

    The first date is the base date rather than the reference date, which is
    passed separately and normally follows it; the first rate is the base rate
    the curve publishes, and node times are measured from the reference date, so
    the first one is negative."""

    def __init__(
        self,
        reference_date: Date,
        dates: list[Date],
        rates: list[float],
        frequency: Frequency,
        day_counter: DayCounter,
    ) -> None:
        """Raises ItofinError on fewer than two dates, a dates/rates count
        mismatch, or a rate at or below -100 % from the second node on - the
        base rate is left unconstrained."""
        ...
    def times(self) -> list[float]: ...
    def dates(self) -> list[Date]: ...
    def nodes(self) -> list[tuple[Date, float]]: ...

class YoYInflationHelper:
    """Shared base for every year-on-year bootstrap helper: the two dates the
    bootstrap places a curve node by.

    Concrete helpers such as YearOnYearInflationSwapHelper subclass this and
    supply only their constructor."""

    def pillar_date(self) -> Date: ...
    def latest_date(self) -> Date: ...

class YearOnYearInflationSwapHelper(YoYInflationHelper):
    """The bootstrap helper fitting a year-on-year inflation swap quoted as a
    rate.

    The helper prices a unit-notional, zero-strike swap of its own and reports
    that contract's fair rate; the bootstrap drives the quoted rate less that
    fair rate to zero. Unlike its zero-coupon twin it does need a nominal curve:
    the year-on-year legs pay on a schedule of dates rather than one, so their
    discount factors do not cancel.

    The swap starts at the evaluation date, so that date must be set before this
    constructor runs, not merely before the bootstrap. It prices through a copy
    of index linked to a handle of its own, so the caller's index need not be
    linked to any curve.

    pillar is accepted for signature parity but never read: it only ever
    discriminates on the interpolated path, which is refused."""

    def __init__(
        self,
        quote: SimpleQuote,
        swap_obs_lag: Period,
        maturity: Date,
        calendar: Calendar,
        payment_convention: BusinessDayConvention,
        day_counter: DayCounter,
        index: YoYInflationIndex,
        interpolation: CpiInterpolationType,
        nominal_term_structure: YieldTermStructure,
        settings: Settings,
        pillar: Pillar = ...,
    ) -> None:
        """Raises ItofinError on CpiInterpolationType.Linear, which the core
        refuses outright pending the interpolated branch (#847), and on an
        observation lag the helper's own swap legs cannot be built under."""
        ...

class PiecewiseYoYInflationCurve(YoYInflationTermStructure):
    """A year-on-year inflation curve bootstrapped from year-on-year helpers,
    solving one rate node per helper fixing period.

    Node zero sits on base_date at base_yoy_rate and is kept rather than solved,
    so times()[0] is negative. Each helper's observed fixing period marks a
    later segment boundary.

    Lazy: the bootstrap runs on the first read, so the evaluation date must be
    in place before that read as well as before the helpers were built. A helper
    quote moving invalidates the cache."""

    def __init__(
        self,
        reference_date: Date,
        base_date: Date,
        base_yoy_rate: float,
        frequency: Frequency,
        day_counter: DayCounter,
        helpers: list[YoYInflationHelper],
    ) -> None:
        """Raises ItofinError on an empty helper list."""
        ...
    def calculate(self) -> None: ...
    def times(self) -> list[float]: ...
    def dates(self) -> list[Date]: ...
    def nodes(self) -> list[tuple[Date, float]]: ...

class ConstantYoYOptionletVolatility:
    """One year-on-year optionlet volatility for every strike and every date.

    The reference date moves with the evaluation date carried by settings,
    settlement_days business days on from it, so that date must be set before
    anything is priced off the surface.

    min_strike and max_strike bound the strike domain a query is answered over;
    C++ defaults them to -1.0 and 100.0 and the port carries no default
    arguments, so both are passed here too.

    Both constructors are bound: __init__ takes a value, with_quote a live
    quote. The whole stripped/interpolated hierarchy is deferred (#874)."""

    def __init__(
        self,
        volatility: float,
        settlement_days: int,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        min_strike: float,
        max_strike: float,
        settings: Settings,
    ) -> None: ...
    @staticmethod
    def with_quote(
        volatility: SimpleQuote,
        settlement_days: int,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        min_strike: float,
        max_strike: float,
        settings: Settings,
    ) -> ConstantYoYOptionletVolatility:
        """A flat surface quoted by volatility: a later set_value on the quote
        notifies the surface's observers, so anything priced off it reprices at
        the new level. Otherwise as __init__."""
        ...
    def observation_lag(self) -> Period: ...
    def frequency(self) -> Frequency: ...
    def index_is_interpolated(self) -> bool: ...
    def base_date(self) -> Date:
        """The date the surface measures its variance from. Raises ItofinError
        on an unset evaluation date, and on a frequency admitting no publication
        period."""
        ...
    def volatility(self, date: Date, strike: float, obs_lag: Period) -> float:
        """The lag is explicit rather than defaulted: pass observation_lag() for
        the surface's own. Raises ItofinError when the observed date falls before
        base_date(), or strike lies outside the strike domain."""
        ...
    def total_variance(self, date: Date, strike: float, obs_lag: Period) -> float:
        """The total integrated variance, the figure that scales time out of the
        optionlet formulae without committing to a distribution. Raises as
        volatility()."""
        ...
