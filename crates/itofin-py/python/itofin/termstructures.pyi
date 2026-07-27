# Hand-written stubs for itofin.termstructures; sync manually with src/curve.rs, src/vol.rs,
# src/helpers.rs, src/swaptionvol.rs, src/optionletvol.rs and src/smilesection.rs (#517).

from itofin import Settings
from itofin.indexes import Estr, Euribor, SwapIndex
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
    """A term structure of Black volatility (no strike dimension), interpolating
    linearly on variance. Finite in time: enable extrapolation past the last date,
    where ``time_extrapolation`` picks the rule."""

    def __init__(
        self,
        reference_date: Date,
        dates: list[Date],
        black_vol_curve: list[float],
        day_counter: DayCounter,
        force_monotone_variance: bool,
        time_extrapolation: BlackVolTimeExtrapolation = ...,
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
        index: Euribor,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> None: ...
    @staticmethod
    def from_rate(
        rate: float,
        period_to_start: Period,
        index: Euribor,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> FraRateHelper: ...
    @staticmethod
    def from_months(
        quote: SimpleQuote,
        months_to_start: int,
        index: Euribor,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> FraRateHelper: ...
    @staticmethod
    def from_dates(
        quote: SimpleQuote,
        start_date: Date,
        end_date: Date,
        index: Euribor,
        use_indexed_coupon: bool = True,
        pillar: Pillar = ...,
    ) -> FraRateHelper: ...

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
        overnight_index: Estr,
        payment_lag: int,
        payment_convention: BusinessDayConvention,
        payment_frequency: Frequency,
        forward_start: Period,
        settings: Settings,
        discounting_curve: YieldTermStructure | None = None,
        overnight_spread: SimpleQuote | None = None,
        pillar: Pillar = ...,
        averaging_method: RateAveraging = ...,
    ) -> None: ...

class SwaptionVolatilityStructure:
    """Shared base for every swaption volatility surface: volatility, Black
    variance and lognormal shift, addressed by option and swap tenor."""

    def volatility(
        self,
        option_tenor: Period,
        swap_tenor: Period,
        strike: float,
        extrapolate: bool = False,
    ) -> float: ...
    def black_variance(
        self,
        option_tenor: Period,
        swap_tenor: Period,
        strike: float,
        extrapolate: bool = False,
    ) -> float: ...
    def shift(
        self, option_date: Date, swap_length: float, extrapolate: bool = False
    ) -> float:
        """The lognormal shift, in the date form: the core has no tenor overload
        for the shift. Errors on a normal-volatility surface."""
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
    ) -> None: ...
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
        """Reads the volatility from the caller's quote; a later set_value
        notifies the surface's observers."""
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
        """Pins the reference date, so every query's option time runs from
        reference_date rather than the evaluation date."""
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
        """A grid whose reference date floats off the evaluation date, reading
        each node from the caller's quote: a later set_value on any of them
        rebuilds the interpolation and notifies the grid's observers."""
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
    ) -> None: ...
    def atm_strike_from_tenor(self, option_tenor: Period, swap_tenor: Period) -> float:
        """The at-the-money strike for an option tenor and swap tenor: the fixing
        of whichever base swap index the swap tenor selects."""
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
        """Raises ItofinError on a non-zero shift or a Normal volatility_type
        (both deferred to #586), on a non-positive shifted forward, and on SABR
        parameters outside alpha > 0, beta in [0, 1], nu >= 0, rho^2 < 1."""
        ...
    def volatility(self, strike: float) -> float: ...
    def variance(self, strike: float) -> float: ...
    @property
    def exercise_time(self) -> float: ...
    @property
    def atm_level(self) -> float: ...
    @property
    def alpha(self) -> float: ...
    @property
    def beta(self) -> float: ...
    @property
    def nu(self) -> float: ...
    @property
    def rho(self) -> float: ...

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
    ) -> None: ...
    def atm_strike_from_tenor(self, option_tenor: Period, swap_tenor: Period) -> float:
        """The at-the-money strike for an option tenor and swap tenor: the strike
        the fitted smile is centred on."""
        ...

class OptionletVolatilityStructure:
    """Shared base for every caplet/floorlet volatility surface: volatility,
    Black variance and the lognormal displacement.

    A single option axis, unlike the swaption surfaces: a query takes one option
    tenor (or date) and a strike."""

    def volatility(
        self, option_tenor: Period, strike: float, extrapolate: bool = False
    ) -> float: ...
    def volatility_date(
        self, option_date: Date, strike: float, extrapolate: bool = False
    ) -> float: ...
    def black_variance(
        self, option_tenor: Period, strike: float, extrapolate: bool = False
    ) -> float: ...
    def allows_extrapolation(self) -> bool: ...
    def enable_extrapolation(self) -> None:
        """A stripped surface ends at its last optionlet fixing, so a cap whose
        own last caplet fixes there queries the boundary."""
        ...
    def disable_extrapolation(self) -> None: ...
    def displacement(self) -> float:
        """The lognormal shift applied to forwards and strikes. This is what
        BlackCapFloorEngine checks a caller-supplied displacement against."""
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
    ) -> None: ...
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
        """Reads the volatility from the caller's quote; a later set_value
        notifies the surface's observers."""
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
        """Raises ItofinError on an empty or ragged grid, on a grid whose shape
        does not match the tenors and strikes, and on a non-increasing tenor or
        strike axis."""
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
        """Reads each node from the caller's quote; a later set_value rebuilds
        the interpolation and notifies the surface's observers."""
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
        """The reference date floats settlement_days business days off the
        evaluation date. This is the form OptionletStripper1 and
        StrippedOptionletAdapter need."""
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
        """The floating-reference surface over the caller's quotes."""
        ...
    def volatility(
        self, option_tenor: Period, strike: float, extrapolate: bool = False
    ) -> float: ...
    def volatility_date(
        self, end_date: Date, strike: float, extrapolate: bool = False
    ) -> float: ...
    def volatility_time(
        self, length: float, strike: float, extrapolate: bool = False
    ) -> float:
        """length is a year fraction off the reference date in the surface's own
        day count."""
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
        ibor_index: Euribor,
        volatility_type: VolatilityType,
        accuracy: float = 1e-6,
        max_iter: int = 100,
        displacement: float = 0.0,
        discount: YieldTermStructure | None = None,
        optionlet_frequency: Period | None = None,
    ) -> None:
        """discount None falls back to the index's own forwarding curve;
        optionlet_frequency None uses the index tenor as the caplet step."""
        ...
    def switch_strike(self) -> float:
        """The mean at-the-money caplet rate, which decides whether each strike
        is stripped out of caps or out of floors. The first call strips."""
        ...
    def atm_optionlet_rates(self) -> list[float]:
        """The at-the-money forward rate of each caplet, one per maturity."""
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
        """Strips eagerly, to snapshot the strike domain and maximum date.
        Raises ItofinError on a stripper whose term-volatility surface carries
        no settlement days."""
        ...
