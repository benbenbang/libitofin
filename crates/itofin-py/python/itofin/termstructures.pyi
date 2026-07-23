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
    """A yield curve interpolating continuously-compounded zero rates linearly
    between nodes. The first date is the reference date; finite in time."""

    def __init__(
        self,
        dates: list[Date],
        yields: list[float],
        day_counter: DayCounter,
    ) -> None: ...

class DiscountCurve(YieldTermStructure):
    """A yield curve interpolating discount factors log-linearly (piecewise-constant
    forwards). The first date is the reference date and its discount must be 1.0."""

    def __init__(
        self,
        dates: list[Date],
        discounts: list[float],
        day_counter: DayCounter,
        calendar: Calendar | None = None,
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
