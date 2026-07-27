# Hand-written stubs for itofin.pricingengines; sync manually with src/swaptionengine.rs (#517).

from itofin import Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import SwaptionVolatilityStructure, YieldTermStructure
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
