# Hand-written stubs for itofin.pricingengines; sync manually with src/swaptionengine.rs
# and src/capfloorengine.rs (#517).

from itofin import Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    OptionletVolatilityStructure,
    SwaptionVolatilityStructure,
    YieldTermStructure,
)
from itofin.time import DayCounter

class CashAnnuityModel:
    """Which date a cash-settled par-yield annuity discounts to. Only the
    (Cash, ParYieldCurve) settlement pair reads it."""

    SwapRate: CashAnnuityModel
    DiscountCurve: CashAnnuityModel

class BlackSwaptionEngine:
    """The shifted-lognormal Black-formula swaption engine, European-only.

    It prices the underlying swap itself, so that swap needs no engine of its
    own. The settings passed here must be the same object driving the swaption
    and its swap: a mismatch prices the two on different evaluation dates with
    no error raised. The surface's volatility type is checked against the Black
    formula at pricing time, not construction."""

    def __init__(
        self,
        vol: SwaptionVolatilityStructure,
        discount: YieldTermStructure,
        settings: Settings,
        model: CashAnnuityModel = ...,
    ) -> None: ...
    @staticmethod
    def with_flat_vol(
        discount: YieldTermStructure,
        vol: SimpleQuote,
        day_counter: DayCounter,
        displacement: float,
        settings: Settings,
        model: CashAnnuityModel = ...,
    ) -> BlackSwaptionEngine:
        """An engine over a flat volatility quote, wrapped internally in a
        constant surface on a null calendar whose reference date tracks the
        evaluation date. displacement is that surface's lognormal shift."""
        ...

class BlackCapFloorEngine:
    """The shifted-lognormal Black-formula cap/floor engine, one Black 1976
    optionlet per coupon.

    Only the shifted-lognormal path is priced in the core, so a normal-volatility
    surface is rejected by the constructor rather than bound to a Bachelier
    engine. The instrument this engine prices must resolve its dates against the
    same Settings object the engine does."""

    def __init__(
        self,
        vol: OptionletVolatilityStructure,
        discount: YieldTermStructure,
        displacement: float | None = None,
    ) -> None:
        """Raises ItofinError on a normal-volatility surface, and when a given
        displacement differs from the surface's own. None adopts the surface's
        displacement."""
        ...
    @staticmethod
    def with_flat_vol(
        discount: YieldTermStructure,
        vol: SimpleQuote,
        day_counter: DayCounter,
        displacement: float,
        settings: Settings,
    ) -> BlackCapFloorEngine:
        """An engine over a flat volatility quote, wrapped internally in a
        constant optionlet surface on a null calendar whose reference date
        tracks the evaluation date. displacement is that surface's lognormal
        shift."""
        ...
    def displacement(self) -> float: ...
