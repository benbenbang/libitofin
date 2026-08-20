# Hand-written stubs for itofin.pricingengines; sync manually with src/swaptionengine.rs,
# src/capfloorengine.rs, src/creditengine.rs, src/inflation.rs and src/mcengine.rs (#517).

# itofin library
from itofin import Settings
from itofin.indexes import YoYInflationIndex
from itofin.processes import BlackScholesProcess, HestonProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    ConstantYoYOptionletVolatility,
    DefaultProbabilityTermStructure,
    OptionletVolatilityStructure,
    SwaptionVolatilityStructure,
    YieldTermStructure,
)
from itofin.time import DayCounter

class CashAnnuityModel:
    """Which date a cash-settled par-yield annuity discounts to.

    Only the (Cash, ParYieldCurve) settlement pair reads it; every other pair
    takes the fixed-leg BPS annuity and is insensitive to the choice. The
    swaption engines default to SwapRate, the branch every ported core test
    exercises, where C++ defaults to DiscountCurve.
    """

    SwapRate: CashAnnuityModel
    DiscountCurve: CashAnnuityModel

class BlackSwaptionEngine:
    """The shifted-lognormal Black-formula swaption engine, European-only.

    It prices the underlying swap itself, so that swap needs no engine of its
    own. The settings passed here must be the same object driving the swaption
    and its swap: a mismatch prices the two on different evaluation dates with
    no error raised. The surface's volatility type is checked against the Black
    formula at pricing time, not construction, so a normal-volatility surface
    raises from Swaption.npv().
    """

    def __init__(
        self,
        vol: SwaptionVolatilityStructure,
        discount: YieldTermStructure,
        settings: Settings,
        model: CashAnnuityModel = ...,
    ) -> None:
        """Build an engine reading volatilities off vol and discounting on discount.

        Args:
            vol (SwaptionVolatilityStructure): The surface volatilities are read
                off.
            discount (YieldTermStructure): The curve both legs are discounted
                on.
            settings (Settings): The explicit settings; must be the same object
                driving the swaption and its swap.
            model (CashAnnuityModel): Which date a cash-settled par-yield
                annuity discounts to. Defaults to SwapRate.
        """
        ...
    @staticmethod
    def with_flat_vol(
        discount: YieldTermStructure,
        vol: SimpleQuote,
        day_counter: DayCounter,
        displacement: float,
        settings: Settings,
        model: CashAnnuityModel = ...,
    ) -> BlackSwaptionEngine:
        """Build an engine over a flat volatility quote.

        The quote is wrapped internally in a constant surface on a null calendar
        whose reference date tracks the evaluation date.

        Args:
            discount (YieldTermStructure): The curve both legs are discounted
                on.
            vol (SimpleQuote): The flat Black volatility.
            day_counter (DayCounter): The day count the constant surface
                measures time on.
            displacement (float): The constant surface's lognormal shift.
            settings (Settings): The explicit settings; must be the same object
                driving the swaption and its swap.
            model (CashAnnuityModel): Which date a cash-settled par-yield
                annuity discounts to. Defaults to SwapRate.

        Returns:
            BlackSwaptionEngine: The engine over the flat surface.
        """
        ...

class BachelierSwaptionEngine:
    """The normal-volatility swaption engine, European-only.

    The Bachelier spec of the template BlackSwaptionEngine instantiates: same
    constructors, same settings requirement, same silent discounting engine on
    the underlying swap. The surface's volatility type is checked against the
    normal formula at pricing time, not construction, so a shifted-lognormal
    surface raises from Swaption.npv().
    """

    def __init__(
        self,
        vol: SwaptionVolatilityStructure,
        discount: YieldTermStructure,
        settings: Settings,
        model: CashAnnuityModel = ...,
    ) -> None:
        """Build an engine reading normal volatilities off vol.

        Args:
            vol (SwaptionVolatilityStructure): The surface normal volatilities
                are read off.
            discount (YieldTermStructure): The curve both legs are discounted
                on.
            settings (Settings): The explicit settings; must be the same object
                driving the swaption and its swap.
            model (CashAnnuityModel): Which date a cash-settled par-yield
                annuity discounts to. Defaults to SwapRate.
        """
        ...
    @staticmethod
    def with_flat_vol(
        discount: YieldTermStructure,
        vol: SimpleQuote,
        day_counter: DayCounter,
        displacement: float,
        settings: Settings,
        model: CashAnnuityModel = ...,
    ) -> BachelierSwaptionEngine:
        """Build an engine over a flat normal volatility quote.

        The quote is wrapped internally in a constant surface on a null calendar
        whose reference date tracks the evaluation date.

        Args:
            discount (YieldTermStructure): The curve both legs are discounted
                on.
            vol (SimpleQuote): The flat normal volatility.
            day_counter (DayCounter): The day count the constant surface
                measures time on.
            displacement (float): Kept for signature parity with the Black
                engine; the normal model has no shift and ignores it.
            settings (Settings): The explicit settings; must be the same object
                driving the swaption and its swap.
            model (CashAnnuityModel): Which date a cash-settled par-yield
                annuity discounts to. Defaults to SwapRate.

        Returns:
            BachelierSwaptionEngine: The engine over the flat surface.
        """
        ...

class BlackCapFloorEngine:
    """The shifted-lognormal Black-formula cap/floor engine, one Black 1976
    optionlet per coupon.

    Only the shifted-lognormal path is priced in the core, so a normal-volatility
    surface is rejected by the constructor rather than bound to a Bachelier
    engine. The instrument this engine prices must resolve its dates against the
    same Settings object the engine does.
    """

    def __init__(
        self,
        vol: OptionletVolatilityStructure,
        discount: YieldTermStructure,
        displacement: float | None = None,
    ) -> None:
        """Build an engine reading volatilities off vol and discounting on discount.

        Fallible at construction, unlike the swaption engines.

        Args:
            vol (OptionletVolatilityStructure): The optionlet surface
                volatilities are read off; must be shifted-lognormal.
            discount (YieldTermStructure): The curve the optionlets are
                discounted on.
            displacement (float | None): The lognormal shift; None adopts the
                surface's own.

        Raises:
            ItofinError: If the surface handle is empty, the surface is
                normal-volatility, or a given displacement differs from the
                surface's own.
        """
        ...
    @staticmethod
    def with_flat_vol(
        discount: YieldTermStructure,
        vol: SimpleQuote,
        day_counter: DayCounter,
        displacement: float,
        settings: Settings,
    ) -> BlackCapFloorEngine:
        """Build an engine over a flat volatility quote.

        The quote is wrapped internally in a constant optionlet surface on a
        null calendar whose reference date tracks the evaluation date.
        displacement carries no default, mirroring the swaption engine: a
        trailing settings cannot follow a defaulted argument.

        Args:
            discount (YieldTermStructure): The curve the optionlets are
                discounted on.
            vol (SimpleQuote): The flat Black volatility.
            day_counter (DayCounter): The day count the constant surface
                measures time on.
            displacement (float): The constant surface's lognormal shift.
            settings (Settings): The explicit settings the constant surface's
                reference date tracks.

        Returns:
            BlackCapFloorEngine: The engine over the flat surface.
        """
        ...
    def displacement(self) -> float:
        """Return the lognormal shift the engine applies to forwards and strikes.

        Returns:
            float: The displacement.
        """
        ...

class MidPointCdsEngine:
    """The mid-point credit-default-swap engine: each live premium period is
    priced against the default probability over that period, with the default
    placed at the period's mid-point.

    Infallible at construction - every precondition (an empty curve handle, an
    unset evaluation date) is reported when the contract is priced. The core's
    include_settlement_date_flows override is not exposed and is always None,
    so the settlement-date flow decision follows the settings' own flags. The
    contract this engine prices must carry the same Settings object.
    """

    def __init__(
        self,
        probability: DefaultProbabilityTermStructure,
        recovery: float,
        discount: YieldTermStructure,
        settings: Settings,
    ) -> None:
        """Build an engine over a default-probability curve and a discount curve.

        Args:
            probability (DefaultProbabilityTermStructure): The curve default
                probabilities are read off.
            recovery (float): The recovery rate; a default pays 1 - recovery of
                the notional.
            discount (YieldTermStructure): The curve both legs are discounted
                on.
            settings (Settings): The explicit settings; must be the same object
                the contract this engine prices was built with.
        """
        ...

class NumericalFix:
    """How the ISDA engine keeps the integrands' f + h denominators away from zero.

    NoFix adds 10^-50 to them instead; Taylor, the default, replaces the
    quotient by its Taylor expansion once f + h falls below 10^-4. Spelled NoFix
    rather than C++'s None, which Python cannot name.
    """

    NoFix: NumericalFix
    Taylor: NumericalFix

class AccrualBias:
    """Whether the premium leg carries the standard model's half-day accrual bias.

    The bias shifts the accrual's tstart back by 1/730 of a year. HalfDayBias,
    the default, includes it as the model's C code does before version 1.8.2;
    NoBias leaves it out, as from 1.8.2 on.
    """

    HalfDayBias: AccrualBias
    NoBias: AccrualBias

class ForwardsInCouponPeriod:
    """How the ISDA engine treats forward rates inside a coupon period.

    Piecewise, the default, subdivides each period at the integration grid's own
    nodes; Flat integrates each period in a single step. The two part only where
    the grid has nodes strictly inside a coupon period, so two flat curves price
    identically under either.
    """

    Flat: ForwardsInCouponPeriod
    Piecewise: ForwardsInCouponPeriod

class IsdaCdsEngine:
    """The ISDA standard-model credit-default-swap engine: both legs are
    integrated over the pillar dates of the two curves the engine is built with
    rather than over the premium schedule alone.

    Infallible at construction, like MidPointCdsEngine. The model is specified
    against curves of a fixed shape, so every check - both curves counting
    Act/365 (Fixed) and referenced at the evaluation date, the contract settling
    its accrual, paying at the default time and carrying a face-value claim - is
    reported as ItofinError when the contract is priced, not from __init__. The
    core's include_settlement_date_flows override is not exposed and is always
    None. The three fidelity flags are trailing keyword arguments defaulting to
    the C++ defaults Taylor / HalfDayBias / Piecewise, so an engine built
    without them prices as before; they are taken here rather than through a
    with_fidelity method because the core builder consumes the engine while
    set_isda_engine has already cloned it into the contract. The contract this
    engine prices must carry the same Settings object.
    """

    def __init__(
        self,
        probability: DefaultProbabilityTermStructure,
        recovery: float,
        discount: YieldTermStructure,
        settings: Settings,
        numerical_fix: NumericalFix = ...,
        accrual_bias: AccrualBias = ...,
        forwards_in_coupon_period: ForwardsInCouponPeriod = ...,
    ) -> None:
        """Build an ISDA standard-model engine under the three fidelity flags.

        Args:
            probability (DefaultProbabilityTermStructure): The curve default
                probabilities are read off; must count Act/365 (Fixed) and be
                referenced at the evaluation date, checked at pricing time.
            recovery (float): The recovery rate; a default pays 1 - recovery of
                the notional.
            discount (YieldTermStructure): The curve both legs are discounted
                on, under the same shape requirement.
            settings (Settings): The explicit settings; must be the same object
                the contract this engine prices was built with.
            numerical_fix (NumericalFix): How the integrand denominators are
                kept away from zero. Defaults to Taylor.
            accrual_bias (AccrualBias): Whether the premium leg carries the
                half-day accrual bias. Defaults to HalfDayBias.
            forwards_in_coupon_period (ForwardsInCouponPeriod): How forward
                rates inside a coupon period are integrated. Defaults to
                Piecewise.
        """
        ...

class DiscountingSwapEngine:
    """Discounts every leg of a swap over a single yield curve.

    Infallible at construction - every precondition (an empty curve handle, an
    unset evaluation date) is reported when the swap is priced. The core's
    include_settlement_date_flows, settlement_date and npv_date overrides are
    not exposed and are always None, so the flow decision follows the settings'
    own flags and both dates fall back to the curve reference date. The swap
    this engine prices must carry the same Settings object.
    """

    def __init__(self, discount: YieldTermStructure, settings: Settings) -> None:
        """Build an engine discounting every leg on discount.

        Args:
            discount (YieldTermStructure): The curve every leg is discounted on;
                the engine registers as an observer of it.
            settings (Settings): The explicit settings; must be the same object
                the swap this engine prices was built with, or the two resolve
                their dates against different evaluation dates and the NPV is
                silently wrong.
        """
        ...

class MCEuropeanEngine:
    """The Monte Carlo engine for European payoffs, over the pseudo-random RNG
    policy. The low-discrepancy policy is not exposed (#454).

    Pricing is seeded and deterministic: the same seed reproduces the NPV
    bitwise, and the standard error is read back through
    VanillaOption.error_estimate().
    """

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
        """Build an engine over process, configured through the core factory.

        Every argument past process is left unset when omitted, so the core's
        own validation reports the illegal combinations.

        Args:
            process (BlackScholesProcess): The process paths are drawn from.
            steps (int | None): The fixed number of time steps per path.
            steps_per_year (int | None): The time steps per year, the
                alternative to steps.
            samples (int | None): The fixed number of paths to draw.
            absolute_tolerance (float | None): The target standard error, the
                alternative to samples.
            max_samples (int | None): The cap on paths drawn when running to a
                tolerance.
            seed (int | None): The RNG seed; the same seed reproduces the NPV
                bitwise.
            antithetic (bool | None): The antithetic variate, not yet supported
                by the core engine (#772).

        Raises:
            ItofinError: If neither or both of steps and steps_per_year are
                given, if both samples and absolute_tolerance are given, or if
                antithetic is True.
        """
        ...

class MCEuropeanHestonEngine:
    """The Monte Carlo engine for European payoffs on a Heston process, over the
    pseudo-random RNG policy. The low-discrepancy policy is not exposed (#454).

    Unlike MCEuropeanEngine, the antithetic variate is supported here.

    Pricing is seeded and deterministic: the same seed reproduces the NPV
    bitwise, and the standard error is read back through
    VanillaOption.error_estimate().
    """

    def __init__(
        self,
        process: HestonProcess,
        steps: int | None = None,
        steps_per_year: int | None = None,
        samples: int | None = None,
        absolute_tolerance: float | None = None,
        max_samples: int | None = None,
        seed: int | None = None,
        antithetic: bool | None = None,
    ) -> None:
        """Build an engine over process, configured through the core factory.

        Every argument past process is left unset when omitted, so the core's
        own validation reports the illegal combinations.

        Args:
            process (HestonProcess): The Heston process paths are drawn from.
            steps (int | None): The fixed number of time steps per path.
            steps_per_year (int | None): The time steps per year, the
                alternative to steps.
            samples (int | None): The fixed number of paths to draw.
            absolute_tolerance (float | None): The target standard error, the
                alternative to samples.
            max_samples (int | None): The cap on paths drawn when running to a
                tolerance.
            seed (int | None): The RNG seed; the same seed reproduces the NPV
                bitwise.
            antithetic (bool | None): The antithetic variate, supported here;
                the core cached oracle prices with it on.

        Raises:
            ItofinError: If neither or both of steps and steps_per_year are
                given, or if both samples and absolute_tolerance are given.
        """
        ...

class MCAmericanEngine:
    """The Longstaff-Schwartz least-squares Monte Carlo engine for American
    payoffs, over the pseudo-random RNG policy. The low-discrepancy policy is
    not exposed (#454), and the Monomial regression basis is not selectable
    (#453).

    The option priced must come from VanillaOption.american(...): a
    European-exercise option raises ItofinError ("wrong exercise given") when
    priced here.

    Pricing is seeded and deterministic: the same seed reproduces the NPV
    bitwise, the standard error is read back through
    VanillaOption.error_estimate() and the early-exercise fraction through
    VanillaOption.exercise_probability().
    """

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
        polynomial_order: int | None = None,
        calibration_samples: int | None = None,
    ) -> None:
        """Build an engine over process, configured through the core factory.

        Every argument past process is left unset when omitted, so the core's
        own validation reports the illegal combinations.

        Args:
            process (BlackScholesProcess): The process paths are drawn from.
            steps (int | None): The fixed number of time steps per path.
            steps_per_year (int | None): The time steps per year, the
                alternative to steps.
            samples (int | None): The fixed number of paths to draw.
            absolute_tolerance (float | None): The target standard error, the
                alternative to samples.
            max_samples (int | None): The cap on paths drawn when running to a
                tolerance.
            seed (int | None): The RNG seed; the same seed reproduces the NPV
                bitwise.
            antithetic (bool | None): The antithetic variate, supported here;
                the core oracle prices with it on.
            polynomial_order (int | None): The order of the Monomial regression
                basis. The core default is 2.
            calibration_samples (int | None): The paths the regression is fitted
                on. The core default is 2048.

        Raises:
            ItofinError: If neither or both of steps and steps_per_year are
                given, or if both samples and absolute_tolerance are given.
        """
        ...

class YoYInflationCapFloorEngine:
    """Prices a year-on-year inflation cap or floor optionlet by optionlet.

    The distribution is chosen by the constructor rather than passed as an
    argument, mirroring C++'s three engine classes: black is lognormal,
    unit_displaced lognormal in 1 + rate and bachelier normal. The core
    YoYOptionletDistribution enum is not bound, so distribution() reads back as
    a string.

    The settings behind the volatility surface and behind the cap/floor this
    engine prices must be the same object, or the two resolve their dates
    against different evaluation dates and the NPV is silently wrong.

    An engine carries the arguments and results of the contract it last priced,
    so a cap and a floor priced together want one engine each.
    """

    @staticmethod
    def black(
        index: YoYInflationIndex,
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure,
    ) -> YoYInflationCapFloorEngine:
        """Build an engine valuing optionlets under the lognormal model.

        Args:
            index (YoYInflationIndex): The index forwards are read off.
            volatility (ConstantYoYOptionletVolatility): The surface optionlet
                volatilities are read off.
            nominal_ts (YieldTermStructure): The nominal curve optionlets are
                discounted on.

        Returns:
            YoYInflationCapFloorEngine: The lognormal engine.
        """
        ...
    @staticmethod
    def unit_displaced(
        index: YoYInflationIndex,
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure,
    ) -> YoYInflationCapFloorEngine:
        """Build an engine valuing optionlets under the unit-displaced lognormal model.

        Lognormal in 1 + rate, the usual quoting convention for a rate that may
        be negative.

        Args:
            index (YoYInflationIndex): The index forwards are read off.
            volatility (ConstantYoYOptionletVolatility): The surface optionlet
                volatilities are read off.
            nominal_ts (YieldTermStructure): The nominal curve optionlets are
                discounted on.

        Returns:
            YoYInflationCapFloorEngine: The unit-displaced lognormal engine.
        """
        ...
    @staticmethod
    def bachelier(
        index: YoYInflationIndex,
        volatility: ConstantYoYOptionletVolatility,
        nominal_ts: YieldTermStructure,
    ) -> YoYInflationCapFloorEngine:
        """Build an engine valuing optionlets under the normal model.

        Args:
            index (YoYInflationIndex): The index forwards are read off.
            volatility (ConstantYoYOptionletVolatility): The surface optionlet
                volatilities are read off.
            nominal_ts (YieldTermStructure): The nominal curve optionlets are
                discounted on.

        Returns:
            YoYInflationCapFloorEngine: The normal engine.
        """
        ...
    def distribution(self) -> str:
        """Return the distribution optionlets are valued under.

        Returns:
            str: "black", "unit_displaced" or "bachelier".
        """
        ...
