# Hand-written stubs for itofin.pricingengines; sync manually with src/swaptionengine.rs,
# src/capfloorengine.rs, src/creditengine.rs, src/inflation.rs and src/mcengine.rs (#517).

from itofin import Settings
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DefaultProbabilityTermStructure,
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

class MidPointCdsEngine:
    """The mid-point credit-default-swap engine: each live premium period is
    priced against the default probability over that period, with the default
    placed at the period's mid-point.

    Infallible at construction - every precondition (an empty curve handle, an
    unset evaluation date) is reported when the contract is priced. The core's
    include_settlement_date_flows override is not exposed and is always None,
    so the settlement-date flow decision follows the settings' own flags. The
    contract this engine prices must carry the same Settings object."""

    def __init__(
        self,
        probability: DefaultProbabilityTermStructure,
        recovery: float,
        discount: YieldTermStructure,
        settings: Settings,
    ) -> None: ...

class DiscountingSwapEngine:
    """Discounts every leg of a swap over a single yield curve.

    Infallible at construction - every precondition (an empty curve handle, an
    unset evaluation date) is reported when the swap is priced. The core's
    include_settlement_date_flows, settlement_date and npv_date overrides are
    not exposed and are always None, so the flow decision follows the settings'
    own flags and both dates fall back to the curve reference date. The swap
    this engine prices must carry the same Settings object."""

    def __init__(self, discount: YieldTermStructure, settings: Settings) -> None: ...

class MCEuropeanEngine:
    """The Monte Carlo engine for European payoffs, over the pseudo-random RNG
    policy. The low-discrepancy policy is not exposed (#454).

    Pricing is seeded and deterministic: the same seed reproduces the NPV
    bitwise, and the standard error is read back through
    VanillaOption.error_estimate()."""

    def __init__(
        self,
        process: BlackScholesProcess,
        steps: int | None = None,
        steps_per_year: int | None = None,
        samples: int | None = None,
        absolute_tolerance: float | None = None,
        max_samples: int | None = None,
        seed: int | None = None,
        antithetic: bool | None = None,
    ) -> None:
        """Raises ItofinError when neither or both of steps / steps_per_year are
        given, when both samples and absolute_tolerance are given, and when
        antithetic is True: the antithetic variate is not yet supported by the
        core engine (#772)."""
        ...
