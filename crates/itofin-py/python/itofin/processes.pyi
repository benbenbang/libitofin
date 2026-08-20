# Hand-written stubs for itofin.processes; sync manually with src/market.rs and
# src/heston.rs (#517).

# itofin library
from itofin.termstructures import BlackVolTermStructure, YieldTermStructure
from itofin.time import Date, DayCounter

class BlackScholesProcess:
    """A generalized Black-Scholes process, built from scalars or curve objects.

    The Handle plumbing is assembled internally, so no handle crosses the
    binding boundary. The constructor takes the conventional
    (risk_free_rate, dividend_yield) order and places the two curves in the
    core's own order at a single call site.
    """

    def __init__(
        self,
        spot: float,
        risk_free_rate: float,
        dividend_yield: float,
        volatility: float,
        reference_date: Date,
        day_counter: DayCounter,
    ) -> None:
        """Build a flat-market process from scalar inputs.

        Args:
            spot (float): The spot level, held as a quote.
            risk_free_rate (float): The flat risk-free rate, made into a curve
                compounded continuously on an annual frequency.
            dividend_yield (float): The flat dividend yield, made into a curve on the
                same convention as the risk-free rate.
            volatility (float): The flat Black volatility.
            reference_date (Date): The date the three flat curves are anchored on.
            day_counter (DayCounter): The day count the curves accrue on.
        """
        ...

    @staticmethod
    def from_curves(
        spot: float,
        risk_free: YieldTermStructure,
        dividend: YieldTermStructure,
        vol: BlackVolTermStructure,
    ) -> BlackScholesProcess:
        """Build a process from term-structure objects instead of scalars.

        The three legs are bound by name and placed in the core's order at a
        single call site, the same risk-free/dividend argument-order footgun the
        scalar constructor guards against.

        Args:
            spot (float): The spot level, held as a quote.
            risk_free (YieldTermStructure): The risk-free discount curve.
            dividend (YieldTermStructure): The dividend curve.
            vol (BlackVolTermStructure): The Black volatility surface.

        Returns:
            BlackScholesProcess: A process over the three supplied term structures.
        """
        ...

    def risk_free_rate(self) -> float:
        """Return the risk-free rate carried by the process.

        Returns:
            float: The continuously compounded zero rate on the risk-free curve at the
            reference date.
        """
        ...

    def dividend_yield(self) -> float:
        """Return the dividend yield carried by the process.

        Returns:
            float: The continuously compounded zero rate on the dividend curve at the
            reference date.
        """
        ...

class HestonProcess:
    """The square-root stochastic-variance process.

    The two flat yield curves and the spot quote are assembled behind their
    handles internally, so no handle crosses the binding boundary.
    """

    def __init__(
        self,
        risk_free_rate: float,
        dividend_yield: float,
        spot: float,
        v0: float,
        kappa: float,
        theta: float,
        sigma: float,
        rho: float,
        reference_date: Date,
        day_counter: DayCounter,
    ) -> None:
        """Build the process from scalar market inputs and the five parameters.

        Args:
            risk_free_rate (float): The flat risk-free rate, made into a curve
                compounded continuously on an annual frequency.
            dividend_yield (float): The flat dividend yield, made into a curve on the
                same convention as the risk-free rate.
            spot (float): The spot level, held as a quote.
            v0 (float): The initial variance.
            kappa (float): The mean-reversion speed.
            theta (float): The long-run variance.
            sigma (float): The volatility of variance.
            rho (float): The spot/variance correlation.
            reference_date (Date): The date the two flat curves are anchored on.
            day_counter (DayCounter): The day count the curves accrue on.
        """
        ...

    def v0(self) -> float:
        """Return the initial variance.

        Returns:
            float: The initial variance v0.
        """
        ...

    def kappa(self) -> float:
        """Return the mean-reversion speed.

        Returns:
            float: The mean-reversion speed kappa.
        """
        ...

    def theta(self) -> float:
        """Return the long-run variance.

        Returns:
            float: The long-run variance theta.
        """
        ...

    def sigma(self) -> float:
        """Return the volatility of variance.

        Returns:
            float: The volatility of variance sigma.
        """
        ...

    def rho(self) -> float:
        """Return the spot/variance correlation.

        Returns:
            float: The spot/variance correlation rho.
        """
        ...
