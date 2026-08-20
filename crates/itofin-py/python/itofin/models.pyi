# Hand-written stubs for itofin.models; sync manually with src/heston.rs,
# src/hullwhite.rs and src/calibration.rs (#517).

# itofin library
from itofin import Settings
from itofin.indexes import IborIndex
from itofin.instruments import OptionType
from itofin.optimization import EndCriteria, LevenbergMarquardt
from itofin.processes import HestonProcess
from itofin.termstructures import YieldTermStructure
from itofin.time import Calendar, Date, DayCounter, Period

class HestonModel:
    """The five-parameter calibrated Heston model.

    The parameters are seeded from the process it is built on and overwritten in
    place by a calibration, so the getters read the fitted values afterwards.
    """

    def __init__(self, process: HestonProcess) -> None:
        """Seed the model from a process.

        Args:
            process (HestonProcess): The process whose five parameters seed the model.

        Raises:
            ItofinError: If a seeded parameter violates its constraint: theta,
                kappa, sigma and v0 must be strictly positive and rho must lie
                in [-1, 1].
        """
        ...

    def theta(self) -> float:
        """Return the long-run variance.

        Returns:
            float: The current value of theta.
        """
        ...

    def kappa(self) -> float:
        """Return the mean-reversion speed.

        Returns:
            float: The current value of kappa.
        """
        ...

    def sigma(self) -> float:
        """Return the volatility of variance.

        Returns:
            float: The current value of sigma.
        """
        ...

    def rho(self) -> float:
        """Return the spot/variance correlation.

        Returns:
            float: The current value of rho.
        """
        ...

    def v0(self) -> float:
        """Return the initial variance.

        Returns:
            float: The current value of v0.
        """
        ...

    def calibrate(
        self,
        helpers: list[HestonModelHelper],
        method: LevenbergMarquardt,
        end_criteria: EndCriteria,
        integration_order: int,
    ) -> None:
        """Fit the five parameters to the helpers and write them back.

        One analytic Heston engine of the given integration order is built on
        this model and installed on every helper, so all helpers price through
        the same engine the optimizer drives. The fitted parameters are readable
        through the getters afterwards.

        Args:
            helpers (list[HestonModelHelper]): The calibration instruments to fit; must not be empty.
            method (LevenbergMarquardt): The optimizer driving the fit.
            end_criteria (EndCriteria): The stopping rule handed to the optimizer.
            integration_order (int): The order of the Gauss-Laguerre integration the
                engine uses; at most 192.

        Raises:
            ItofinError: If integration_order exceeds 192, if helpers is empty,
                or if the optimization itself fails.
        """
        ...

class HullWhite:
    """The one-factor Hull-White short-rate model.

    Fitted to the term structure it is built on; a calibration overwrites a and
    sigma in place, so the getters read the fitted values afterwards.
    """

    def __init__(self, curve: YieldTermStructure, a: float, sigma: float) -> None:
        """Fit the model to a term structure.

        Args:
            curve (YieldTermStructure): The term structure the model fits; its forward rate at 0 is
                read at construction.
            a (float): The mean-reversion speed, under the Vasicek positivity
                constraint.
            sigma (float): The short-rate volatility, under the same constraint.

        Raises:
            ItofinError: If the curve is empty or a parameter violates its
                constraint.
        """
        ...

    def a(self) -> float:
        """Return the mean-reversion speed.

        Returns:
            float: The current value of a, read as the first calibrated-model
            parameter.
        """
        ...

    def sigma(self) -> float:
        """Return the short-rate volatility.

        Returns:
            float: The current value of sigma, read as the second calibrated-model
            parameter.
        """
        ...

    def r0(self) -> float:
        """Return the fitted initial short rate.

        Returns:
            float: The short rate r0 implied by the fitted term structure.
        """
        ...

    def discount_bond_option(
        self,
        option_type: OptionType,
        strike: float,
        maturity: float,
        bond_maturity: float,
    ) -> float:
        """Price a European option on a zero-coupon bond.

        Args:
            option_type (OptionType): Call or put.
            strike (float): The option strike, as a bond price.
            maturity (float): The option expiry, as a time in years.
            bond_maturity (float): The maturity of the underlying zero-coupon bond, as a
                time in years.

        Returns:
            float: The option price.

        Raises:
            ItofinError: If the fitted curve is not linked or the arguments are
                rejected by the underlying Black formula.
        """
        ...

    def calibrate(
        self,
        helpers: list[SwaptionHelper],
        method: LevenbergMarquardt,
        end_criteria: EndCriteria,
        fix_reversion: bool,
    ) -> None:
        """Fit a and sigma to the helpers and write them back.

        One Jamshidian swaption engine is built on this model and installed on
        every helper, so all swaptions price through the same analytic engine
        the optimizer drives.

        Args:
            helpers (list[SwaptionHelper]): The calibration instruments to fit; must not be empty.
            method (LevenbergMarquardt): The optimizer driving the fit.
            end_criteria (EndCriteria): The stopping rule handed to the optimizer.
            fix_reversion (bool): Pin the mean reversion a and free only sigma; when
                False both parameters are free.

        Raises:
            ItofinError: If helpers is empty or the optimization itself fails.
        """
        ...

class HestonModelHelper:
    """A Black-vol calibration helper over a flat-vol surface.

    Assembles its own volatility quote and two flat curves from the scalar
    market inputs, so no handle crosses the binding boundary.
    """

    def __init__(
        self,
        maturity: Period,
        calendar: Calendar,
        s0: float,
        strike: float,
        volatility: float,
        risk_free_rate: float,
        dividend_yield: float,
        error_type: CalibrationErrorType,
        reference_date: Date,
        day_counter: DayCounter,
        settings: Settings,
    ) -> None:
        """Build the helper from scalar market inputs.

        Args:
            maturity (Period): The option tenor.
            calendar (Calendar): The calendar the maturity rolls on.
            s0 (float): The spot level.
            strike (float): The option strike.
            volatility (float): The market Black volatility, held as a quote.
            risk_free_rate (float): The flat risk-free rate, made into a curve compounded
                continuously on an annual frequency.
            dividend_yield (float): The flat dividend yield, made into a curve on the
                same convention as the risk-free rate.
            error_type (CalibrationErrorType): How the market and model prices are compared.
            reference_date (Date): The date the two flat curves are anchored on; it is
                used only to assemble them, not forwarded to the core.
            day_counter (DayCounter): The day count the curves accrue on, used the same way.
            settings (Settings): The evaluation-date store the helper reads.
        """
        ...

    def calibration_error(self) -> float:
        """Return the error between the market and model values.

        Meaningful once a calibration has installed a pricing engine on the
        helper; the comparison follows the helper's error type.

        Returns:
            float: The calibration error under the configured error type.

        Raises:
            ItofinError: If the market or model valuation fails, or the implied
                volatility solve does.
        """
        ...

class SwaptionHelper:
    """A co-terminal swaption calibration instrument.

    Builds its own European swaption from the maturity, length and index, so no
    swap or swaption object is needed. The swaption is struck at the forward on
    shifted-lognormal volatility with zero shift, takes the index's own
    settlement days, and compounds its averaging.
    """

    def __init__(
        self,
        maturity: Period,
        length: Period,
        volatility: float,
        index: IborIndex,
        fixed_leg_tenor: Period,
        fixed_leg_day_counter: DayCounter,
        floating_leg_day_counter: DayCounter,
        curve: YieldTermStructure,
        error_type: CalibrationErrorType,
        nominal: float,
    ) -> None:
        """Build the helper and the swaption underlying it.

        Args:
            maturity (Period): The option tenor, the time to the swaption expiry.
            length (Period): The tenor of the underlying swap.
            volatility (float): The market volatility, held as a quote.
            index (IborIndex): The index the floating leg fixes on.
            fixed_leg_tenor (Period): The payment tenor of the fixed leg.
            fixed_leg_day_counter (DayCounter): The day count the fixed leg accrues on.
            floating_leg_day_counter (DayCounter): The day count the floating leg accrues on.
            curve (YieldTermStructure): The discount curve.
            error_type (CalibrationErrorType): How the market and model prices are compared.
            nominal (float): The notional of the underlying swap.
        """
        ...

    def calibration_error(self) -> float:
        """Return the error between the market and model values.

        Meaningful once a calibration has installed a pricing engine on the
        helper; the comparison follows the helper's error type.

        Returns:
            float: The calibration error under the configured error type.

        Raises:
            ItofinError: If the market or model valuation fails, or the implied
                volatility solve does.
        """
        ...

class CalibrationErrorType:
    """How market and model prices are compared during calibration.

    RelativePriceError is |market - model| / market, PriceError is
    market - model, and ImpliedVolError compares the two implied volatilities.
    """

    RelativePriceError: CalibrationErrorType
    PriceError: CalibrationErrorType
    ImpliedVolError: CalibrationErrorType
