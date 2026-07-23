# Hand-written stubs for itofin.processes; sync manually with src/market.rs and
# src/heston.rs (#517).

from itofin.time import Date, DayCounter

class BlackScholesProcess:
    """A flat-market generalized Black-Scholes process."""

    def __init__(
        self,
        spot: float,
        risk_free_rate: float,
        dividend_yield: float,
        volatility: float,
        reference_date: Date,
        day_counter: DayCounter,
    ) -> None: ...
    def risk_free_rate(self) -> float: ...
    def dividend_yield(self) -> float: ...

class HestonProcess:
    """The square-root stochastic-variance process."""

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
    ) -> None: ...
    def v0(self) -> float: ...
    def kappa(self) -> float: ...
    def theta(self) -> float: ...
    def sigma(self) -> float: ...
    def rho(self) -> float: ...
