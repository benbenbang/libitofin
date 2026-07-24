# Hand-written stubs for itofin.termstructures; sync manually with src/curve.rs, src/vol.rs and src/helpers.rs (#517).

from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Period,
)

class YieldTermStructure:
    """Shared base for every yield curve: discount factors, zero and forward rates."""

    def discount(self, t: float, extrapolate: bool = False) -> float: ...
    def discount_date(self, date: Date, extrapolate: bool = False) -> float: ...
    def zero_rate(self, t: float, extrapolate: bool = False) -> float: ...
    def forward_rate(self, t1: float, t2: float, extrapolate: bool = False) -> float: ...
    def reference_date(self) -> Date: ...
    def max_date(self) -> Date: ...
    def allows_extrapolation(self) -> bool: ...
    def enable_extrapolation(self) -> None: ...
    def disable_extrapolation(self) -> None: ...

class BlackVolTermStructure:
    """Shared base for every Black-volatility surface: spot and forward vol/variance."""

    def black_vol(self, t: float, strike: float, extrapolate: bool = False) -> float: ...
    def black_vol_date(self, date: Date, strike: float, extrapolate: bool = False) -> float: ...
    def black_variance(self, t: float, strike: float, extrapolate: bool = False) -> float: ...
    def black_variance_date(self, date: Date, strike: float, extrapolate: bool = False) -> float: ...
    def black_forward_vol(self, t1: float, t2: float, strike: float, extrapolate: bool = False) -> float: ...
    def black_forward_variance(self, t1: float, t2: float, strike: float, extrapolate: bool = False) -> float: ...
    def min_strike(self) -> float: ...
    def max_strike(self) -> float: ...
    def max_date(self) -> Date: ...
    def allows_extrapolation(self) -> bool: ...
    def enable_extrapolation(self) -> None: ...
    def disable_extrapolation(self) -> None: ...

class FlatForward(YieldTermStructure):
    """A flat continuously-compounded yield curve behind a Handle."""

    def __init__(self, reference_date: Date, rate: float, day_counter: DayCounter) -> None: ...

class ZeroCurve(YieldTermStructure):
    """A yield curve interpolating continuously-compounded zero rates between
    nodes. The first date is the reference date; finite in time. interpolation is
    "Linear" (default) or "Cubic"."""

    def __init__(
        self,
        dates: list[Date],
        yields: list[float],
        day_counter: DayCounter,
        interpolation: str = "Linear",
    ) -> None: ...

class DiscountCurve(YieldTermStructure):
    """A yield curve interpolating discount factors between nodes. The first date
    is the reference date and its discount must be 1.0. interpolation is
    "LogLinear" (default, piecewise-constant forwards) or "Cubic"."""

    def __init__(
        self,
        dates: list[Date],
        discounts: list[float],
        day_counter: DayCounter,
        calendar: Calendar | None = None,
        interpolation: str = "LogLinear",
    ) -> None: ...

class ForwardCurve(YieldTermStructure):
    """A yield curve interpolating instantaneous forward rates backward-flat.
    The first date is the reference date; finite in time."""

    def __init__(
        self,
        dates: list[Date],
        forwards: list[float],
        day_counter: DayCounter,
    ) -> None: ...

class PiecewiseYieldCurve(YieldTermStructure):
    """A yield curve bootstrapped from a strip of rate helpers, one node per
    helper maturity. The bootstrap is lazy: it runs on the first query, not at
    construction. interpolation is "LogLinear" (default) or "Linear"."""

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
        interpolation: str = "LogLinear",
    ) -> None: ...

class PiecewiseLogLinearDiscount(YieldTermStructure):
    """A curve bootstrapped in discount-factor space with log-linear interpolation
    (PiecewiseYieldCurve<Discount, LogLinear>). data() are discount factors, so
    data()[0] is the reference node's 1.0."""

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None: ...
    def dates(self) -> list[Date]: ...
    def data(self) -> list[float]: ...

class PiecewiseLinearZero(YieldTermStructure):
    """A curve bootstrapped in zero-rate space with linear interpolation
    (PiecewiseYieldCurve<ZeroYield, Linear>). data() are zero rates."""

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None: ...
    def dates(self) -> list[Date]: ...
    def data(self) -> list[float]: ...

class PiecewiseLinearForward(YieldTermStructure):
    """A curve bootstrapped in instantaneous forward-rate space with linear
    interpolation (PiecewiseYieldCurve<ForwardRate, Linear>). data() are forward
    rates."""

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None: ...
    def dates(self) -> list[Date]: ...
    def data(self) -> list[float]: ...

class PiecewiseFlatForward(YieldTermStructure):
    """A curve bootstrapped in instantaneous forward-rate space with backward-flat
    interpolation (PiecewiseYieldCurve<ForwardRate, BackwardFlat>). Numerically
    identical to PiecewiseLogLinearDiscount under every query; only data() (forward
    rates vs discount factors) tells them apart."""

    def __init__(
        self,
        reference_date: Date,
        helpers: list[RateHelper],
        day_counter: DayCounter,
    ) -> None: ...
    def dates(self) -> list[Date]: ...
    def data(self) -> list[float]: ...

class BlackConstantVol(BlackVolTermStructure):
    """A flat Black volatility, constant in strike and time."""

    def __init__(
        self,
        reference_date: Date,
        volatility: float,
        day_counter: DayCounter,
        calendar: Calendar | None = None,
    ) -> None: ...

class BlackVarianceCurve(BlackVolTermStructure):
    """A term structure of Black volatility (no strike dimension), interpolating
    linearly on variance. Finite in time: enable extrapolation past the last date."""

    def __init__(
        self,
        reference_date: Date,
        dates: list[Date],
        black_vol_curve: list[float],
        day_counter: DayCounter,
        force_monotone_variance: bool,
    ) -> None: ...

class BlackVarianceSurface(BlackVolTermStructure):
    """A Black volatility surface in strike and expiry, interpolating bilinearly
    on variance. black_vol_matrix has one row per strike and one column per date."""

    def __init__(
        self,
        reference_date: Date,
        dates: list[Date],
        strikes: list[float],
        black_vol_matrix: list[list[float]],
        day_counter: DayCounter,
        calendar: Calendar | None = None,
    ) -> None: ...

class RateHelper:
    """Shared base for every bootstrap helper: implied/market quotes and dates."""

    def implied_quote(self) -> float: ...
    def quote_error(self) -> float: ...
    def quote_value(self) -> float: ...
    def maturity_date(self) -> Date: ...
    def pillar_date(self) -> Date: ...
    def earliest_date(self) -> Date: ...
    def latest_date(self) -> Date: ...
    def latest_relevant_date(self) -> Date: ...

class DepositRateHelper(RateHelper):
    """A helper fitting a deposit rate."""

    def __init__(self, quote: SimpleQuote, index: Euribor) -> None: ...
    @staticmethod
    def from_rate(rate: float, index: Euribor) -> DepositRateHelper: ...

class SwapRateHelper(RateHelper):
    """A helper fitting a par swap rate (spot-starting, no spread)."""

    def __init__(
        self,
        quote: SimpleQuote,
        tenor: Period,
        calendar: Calendar,
        fixed_frequency: Frequency,
        fixed_convention: BusinessDayConvention,
        fixed_day_count: DayCounter,
        ibor_index: Euribor,
    ) -> None: ...

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
    """A helper fitting an exchange-traded interest-rate future's quoted price at
    a fixed IMM/ASX window. The window is absolute (never rebuilt on an
    evaluation-date change). Pass conv_adj=None for an empty (zero) convexity
    adjustment."""

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
    ) -> None: ...
    @staticmethod
    def from_end_date(
        price: SimpleQuote,
        ibor_start_date: Date,
        ibor_end_date: Date | None,
        day_counter: DayCounter,
        conv_adj: SimpleQuote | None,
        futures_type: FuturesType,
    ) -> FuturesRateHelper: ...
    @staticmethod
    def from_index(
        price: SimpleQuote,
        ibor_start_date: Date,
        index: Euribor,
        conv_adj: SimpleQuote | None,
        futures_type: FuturesType,
    ) -> FuturesRateHelper: ...
    def convexity_adjustment(self) -> float: ...
