//! The Black-Scholes wrapper around the one-dimensional solver.
//!
//! Port of `ql/methods/finitedifferences/solvers/fdmblackscholessolver.hpp:40`
//! and its `.cpp`.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::methods::finitedifferences::operators::{FdmBlackScholesOp, FdmLinearOpComposite};
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::types::{Real, Size};

use super::{Fdm1DimSolver, FdmSchemeDesc, FdmSolverDesc};

/// The direction of the mesh the generator is built over (`cpp:49`).
const DIRECTION: Size = 0;

/// Builds the Black-Scholes generator over a mesh, rolls it back through
/// [`Fdm1DimSolver`] and answers value and greeks at a spot.
///
/// The grid is laid out in `ln(S)`, so every accessor takes a spot and passes
/// its logarithm down (`cpp:57-75`); delta and gamma carry the chain rule for
/// that change of variable.
///
/// C++ is a `LazyObject` registered with the process (`hpp:40`, `cpp:41-42`)
/// and this is not. The engine of #668 builds a solver fresh inside its own
/// calculation (`fdblackscholesvanillaengine.cpp:202-207`) and drops it again,
/// so no notification could ever reach one; what the laziness buys is the
/// compute-once behaviour within a single pricing, and that is a plain
/// internal cache.
///
/// Deferred to #636, and omitted rather than accepted and ignored:
///
/// - the local-volatility branch, the `localVol` flag and its
///   `illegalLocalVolOverwrite` fallback (`cpp:49`);
/// - the quanto branch and the `FdmQuantoHelper` it adjusts the drift through
///   (`cpp:50-52`); that helper is not ported.
///
/// [`FdmBlackScholesOp`] takes neither, so both are absent from this
/// constructor rather than accepted and dropped on the floor.
pub struct FdmBlackScholesSolver {
    process: Shared<GeneralizedBlackScholesProcess>,
    strike: Real,
    solver_desc: FdmSolverDesc,
    scheme_desc: FdmSchemeDesc,
    solver: RefCell<Option<Fdm1DimSolver>>,
}

impl FdmBlackScholesSolver {
    /// The solver pricing off `process` at `strike`, over the grid
    /// `solver_desc` describes and under the scheme `scheme_desc` names
    /// (`cpp:30-43`).
    ///
    /// C++ defaults the scheme to `FdmSchemeDesc::Douglas()` (`hpp:45`); the
    /// scheme is named explicitly here, as its one caller already does.
    ///
    /// The process is held by [`Shared`] rather than C++'s
    /// `Handle<GeneralizedBlackScholesProcess>` (`hpp:59`). The handle buys
    /// relinking, which reaches C++ through the observer registration this port
    /// does not carry.
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        strike: Real,
        solver_desc: FdmSolverDesc,
        scheme_desc: FdmSchemeDesc,
    ) -> Self {
        FdmBlackScholesSolver {
            process,
            strike,
            solver_desc,
            scheme_desc,
            solver: RefCell::new(None),
        }
    }

    /// The option value at spot `s` (`cpp:57-60`).
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn value_at(&self, s: Real) -> QlResult<Real> {
        self.with_solver(|solver| solver.interpolate_at(s.ln()))
    }

    /// The delta at spot `s` (`cpp:62-65`), the grid's derivative in `ln(S)`
    /// over `s`.
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn delta_at(&self, s: Real) -> QlResult<Real> {
        self.with_solver(|solver| Ok(solver.derivative_x(s.ln())? / s))
    }

    /// The gamma at spot `s` (`cpp:67-71`), the second derivative in `ln(S)`
    /// less the first, over `s` squared.
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn gamma_at(&self, s: Real) -> QlResult<Real> {
        self.with_solver(|solver| {
            let x = s.ln();

            Ok((solver.derivative_xx(x)? - solver.derivative_x(x)?) / (s * s))
        })
    }

    /// The theta at spot `s` (`cpp:73-75`).
    ///
    /// [`None`] is C++'s `Null<Real>`, which
    /// [`Fdm1DimSolver::theta_at`] returns when the capture would sit on today.
    ///
    /// A deliberate divergence from `cpp:74`: C++ dereferences `solver_`
    /// without calling `calculate()` first, unlike the other three accessors,
    /// and so relies on the engine's fixed value, delta, gamma, theta call
    /// order (`fdblackscholesvanillaengine.cpp:211-214`) having built the
    /// solver already. Ported literally that would leave a caller who asks for
    /// the theta first reading an absent solver. This builds it like the other
    /// three; the numbers are unchanged, only the order they may be asked in.
    ///
    /// # Errors
    ///
    /// Returns an error if the generator cannot be built or the rollback fails.
    pub fn theta_at(&self, s: Real) -> QlResult<Option<Real>> {
        self.with_solver(|solver| solver.theta_at(s.ln()))
    }

    /// Builds the generator and the solver over it, once (`cpp:45-55`).
    fn calculate(&self) -> QlResult<()> {
        if self.solver.borrow().is_some() {
            return Ok(());
        }

        let op = shared_mut(FdmBlackScholesOp::new(
            Shared::clone(&self.solver_desc.mesher),
            &self.process,
            self.strike,
            DIRECTION,
        )?) as SharedMut<dyn FdmLinearOpComposite>;
        let solver = Fdm1DimSolver::new(self.solver_desc.clone(), self.scheme_desc, op);

        *self.solver.borrow_mut() = Some(solver);

        Ok(())
    }

    fn with_solver<T>(&self, read: impl FnOnce(&Fdm1DimSolver) -> QlResult<T>) -> QlResult<T> {
        self.calculate()?;

        let cached = self.solver.borrow();
        let solver = cached.as_ref().expect("calculate leaves the solver built");

        read(solver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::handle::Handle;
    use crate::instruments::PlainVanillaPayoff;
    use crate::interestrate::Compounding;
    use crate::methods::finitedifferences::meshers::FdmMesher;
    use crate::methods::finitedifferences::schemes::testops;
    use crate::methods::finitedifferences::stepconditions::FdmStepConditionComposite;
    use crate::methods::finitedifferences::utilities::fdm_log_inner_value;
    use crate::option::OptionType::Call;
    use crate::payoff::Payoff;
    use crate::quotes::make_quote_handle;
    use crate::shared::shared;
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounter::DayCounter;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::types::{Rate, Time, Volatility};

    const R: Rate = 0.05;
    const Q: Rate = 0.02;
    const VOL: Volatility = 0.2;
    const STRIKE: Real = 100.0;
    const MATURITY: Time = 0.75;
    const STEPS: Size = 10;
    const DAMPING_STEPS: Size = 2;

    /// A spot inside the grid: [`testops::mesher`] spans `(4.0, 5.0)` in
    /// `ln(S)`, so the read-offs are interpolations rather than extrapolations
    /// for any spot between `e^4` and `e^5`.
    const SPOT: Real = 90.0;

    fn process() -> Shared<GeneralizedBlackScholesProcess> {
        let dc = Actual365Fixed::new();
        let today = Date::new(11, Month::February, 2018);

        shared(GeneralizedBlackScholesProcess::new(
            make_quote_handle(100.0).handle(),
            flat_rate(today, Q, dc.clone()),
            flat_rate(today, R, dc.clone()),
            flat_vol(today, VOL, dc),
        ))
    }

    fn flat_rate(reference: Date, rate: Rate, dc: DayCounter) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            rate,
            dc,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn flat_vol(
        reference: Date,
        vol: Volatility,
        dc: DayCounter,
    ) -> Handle<dyn BlackVolTermStructure> {
        Handle::new(shared(BlackConstantVol::new(reference, None, vol, dc))
            as Shared<dyn BlackVolTermStructure>)
    }

    /// A call struck inside the grid, so the seed carries the kink and the
    /// rolled grid is curved where it is read.
    fn desc(mesher: &Shared<dyn FdmMesher>) -> FdmSolverDesc {
        let payoff = shared(PlainVanillaPayoff::new(Call, STRIKE)) as Shared<dyn Payoff>;

        FdmSolverDesc {
            mesher: Shared::clone(mesher),
            bc_set: Vec::new(),
            condition: shared(FdmStepConditionComposite::new(&[], Vec::new())),
            calculator: shared(fdm_log_inner_value(payoff, Shared::clone(mesher), 0)),
            maturity: MATURITY,
            time_steps: STEPS,
            damping_steps: DAMPING_STEPS,
        }
    }

    fn solver(mesher: &Shared<dyn FdmMesher>) -> FdmBlackScholesSolver {
        FdmBlackScholesSolver::new(process(), STRIKE, desc(mesher), FdmSchemeDesc::douglas())
    }

    /// The same wiring by hand: the generator of #656 over the same mesh and
    /// process, handed to the driver of #666 with the same descriptor. Every
    /// number the wrapper answers has to come off this, which is what
    /// separates the transforms from the raw log-space read-offs.
    fn driven_by_hand(mesher: &Shared<dyn FdmMesher>) -> Fdm1DimSolver {
        let op = shared_mut(
            FdmBlackScholesOp::new(Shared::clone(mesher), &process(), STRIKE, DIRECTION).unwrap(),
        ) as SharedMut<dyn FdmLinearOpComposite>;

        Fdm1DimSolver::new(desc(mesher), FdmSchemeDesc::douglas(), op)
    }

    /// `cpp:57-60`: the value is the driver's read-off at the log of the spot,
    /// which pins that the wrapper takes a spot and the driver a log-spot.
    #[test]
    fn the_value_is_the_read_off_at_the_log_of_the_spot() {
        let mesher = testops::mesher();

        assert_eq!(
            solver(&mesher).value_at(SPOT).unwrap(),
            driven_by_hand(&mesher).interpolate_at(SPOT.ln()).unwrap()
        );
    }

    /// The two log-space derivatives at the probe are non-zero and differ from
    /// each other, without which the chain rules below could not fail: a gamma
    /// reading `dxx / s^2` or the subtraction the wrong way round would pass,
    /// and a delta dividing by `s^2` would too.
    #[test]
    fn the_probe_tells_the_two_derivatives_apart() {
        let mesher = testops::mesher();
        let hand = driven_by_hand(&mesher);

        let dx = hand.derivative_x(SPOT.ln()).unwrap();
        let dxx = hand.derivative_xx(SPOT.ln()).unwrap();

        assert!(dx.abs() > 1e-3, "the first derivative is degenerate: {dx}");
        assert!(
            dxx.abs() > 1e-3,
            "the second derivative is degenerate: {dxx}"
        );
        assert!(
            (dxx - dx).abs() > 1e-3,
            "the two derivatives do not discriminate: {dx} against {dxx}"
        );
    }

    /// `cpp:62-65`: the delta is the log-space first derivative over the spot.
    #[test]
    fn the_delta_carries_the_chain_rule_for_the_log_grid() {
        let mesher = testops::mesher();
        let hand = driven_by_hand(&mesher);

        assert_eq!(
            solver(&mesher).delta_at(SPOT).unwrap(),
            hand.derivative_x(SPOT.ln()).unwrap() / SPOT
        );
    }

    /// `cpp:67-71`: the gamma is the second log-space derivative less the
    /// first, over the spot squared.
    #[test]
    fn the_gamma_carries_the_second_chain_rule_term() {
        let mesher = testops::mesher();
        let hand = driven_by_hand(&mesher);

        let expected = (hand.derivative_xx(SPOT.ln()).unwrap()
            - hand.derivative_x(SPOT.ln()).unwrap())
            / (SPOT * SPOT);

        assert_eq!(solver(&mesher).gamma_at(SPOT).unwrap(), expected);
    }

    /// `cpp:73-75`: the theta is the driver's, at the log of the spot and
    /// untransformed - time is not the variable the grid was changed in.
    #[test]
    fn the_theta_is_the_read_off_at_the_log_of_the_spot() {
        let mesher = testops::mesher();

        let expected = driven_by_hand(&mesher)
            .theta_at(SPOT.ln())
            .unwrap()
            .expect("a capture away from today has a theta");

        assert_eq!(
            solver(&mesher)
                .theta_at(SPOT)
                .unwrap()
                .expect("a capture away from today has a theta"),
            expected
        );
    }

    /// The divergence from `cpp:74`, where the theta reads `solver_` without
    /// building it and only the engine's call order saves it: asked first, the
    /// theta is the same number it is when the other three have run before it.
    ///
    /// Equality is exact because both wrappers build the same generator over
    /// the same descriptor, so the divergence is pinned to be about when the
    /// solver is built and not about what it answers.
    #[test]
    fn the_theta_asked_first_is_the_theta_asked_last() {
        let mesher = testops::mesher();

        let asked_first = solver(&mesher)
            .theta_at(SPOT)
            .unwrap()
            .expect("a capture away from today has a theta");

        let in_order = solver(&mesher);
        in_order.value_at(SPOT).unwrap();
        in_order.delta_at(SPOT).unwrap();
        in_order.gamma_at(SPOT).unwrap();

        assert_eq!(
            in_order
                .theta_at(SPOT)
                .unwrap()
                .expect("a capture away from today has a theta"),
            asked_first
        );
    }
}
