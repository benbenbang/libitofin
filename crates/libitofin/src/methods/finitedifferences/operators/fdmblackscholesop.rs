//! The Black-Scholes finite-difference generator.
//!
//! Port of `ql/methods/finitedifferences/operators/fdmblackscholesop.hpp:38`
//! and its `.cpp:32-137`.

use crate::errors::QlResult;
use crate::interestrate::Compounding;
use crate::math::array::Array;
use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::Shared;
use crate::termstructures::volatility::BlackVolTermStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::frequency::Frequency;
use crate::types::{Real, Size, Time};

use super::fdmlinearop::FdmLinearOp;
use super::fdmlinearopcomposite::FdmLinearOpComposite;
use super::firstderivativeop::first_derivative_op;
use super::secondderivativeop::second_derivative_op;
use super::triplebandlinearop::TripleBandLinearOp;

/// The Black-Scholes generator in `ln(S)` over one direction of a mesh.
///
/// [`set_time`](FdmLinearOpComposite::set_time) fills the operator with
/// `(r - q - v/2) D1 + (v/2) D2 - r I` for the step it is given, where `D1`
/// and `D2` are the mesh's first and second derivative operators along
/// `direction` and `v` is the forward variance rate over the step
/// (`cpp:80-97`).
///
/// The curves are read out of the process once, when the operator is built, as
/// in C++ where they are `const shared_ptr` members taken from
/// `currentLink()` (`cpp:40-42`). Relinking a handle of the process afterwards
/// therefore does not reach this operator.
///
/// Deferred to #636, and omitted rather than accepted and ignored:
///
/// - the local-volatility branch, with the `localVol` flag, the
///   `illegalLocalVolOverwrite` fallback and the `x_` grid of spots it needs
///   (`cpp:43-45`, `cpp:55-79`);
/// - the quanto branch, which adjusts the drift through an `FdmQuantoHelper`
///   (`cpp:72-79`, `cpp:84-91`); that helper is not ported;
/// - `toMatrixDecomp` (`cpp:133-135`), which returns a `SparseMatrix`.
pub struct FdmBlackScholesOp {
    mesher: Shared<dyn FdmMesher>,
    r_ts: Shared<dyn YieldTermStructure>,
    q_ts: Shared<dyn YieldTermStructure>,
    vol_ts: Shared<dyn BlackVolTermStructure>,
    dx_map: TripleBandLinearOp,
    dxx_map: TripleBandLinearOp,
    map_t: TripleBandLinearOp,
    strike: Real,
    direction: Size,
}

impl FdmBlackScholesOp {
    /// The generator over `direction` of `mesher`, reading its rates and
    /// volatility from `process` and its forward variance at `strike`
    /// (`cpp:32-49`).
    ///
    /// The operator is unusable until
    /// [`set_time`](FdmLinearOpComposite::set_time) has filled it: its bands
    /// start at zero, as C++'s do at `mapT_(direction, mesher)`.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the process's curves is an empty handle.
    pub fn new(
        mesher: Shared<dyn FdmMesher>,
        process: &GeneralizedBlackScholesProcess,
        strike: Real,
        direction: Size,
    ) -> QlResult<Self> {
        Ok(FdmBlackScholesOp {
            r_ts: process.risk_free_rate().current_link()?,
            q_ts: process.dividend_yield().current_link()?,
            vol_ts: process.black_volatility().current_link()?,
            dx_map: first_derivative_op(direction, Shared::clone(&mesher)),
            dxx_map: second_derivative_op(direction, Shared::clone(&mesher)),
            map_t: TripleBandLinearOp::new(direction, Shared::clone(&mesher)),
            mesher,
            strike,
            direction,
        })
    }
}

impl FdmLinearOp for FdmBlackScholesOp {
    /// `cpp:102-104`.
    fn apply(&self, r: &Array) -> Array {
        self.map_t.apply(r)
    }
}

impl FdmLinearOpComposite for FdmBlackScholesOp {
    /// `cpp:100`: one direction carries the whole operator.
    fn size(&self) -> Size {
        1
    }

    /// `cpp:80-97`, the plain path.
    ///
    /// The two scalars scaling the whole grid are passed as one-element arrays,
    /// which [`axpyb`](TripleBandLinearOp::axpyb) broadcasts, while
    /// [`mult`](TripleBandLinearOp::mult) needs one entry per grid point and so
    /// takes the variance term at full length - the same asymmetry as C++
    /// (`cpp:93-95`).
    fn set_time(&mut self, t1: Time, t2: Time) -> QlResult<()> {
        let r = self
            .r_ts
            .forward_rate(t1, t2, Compounding::Continuous, Frequency::Annual, false)?
            .rate();
        let q = self
            .q_ts
            .forward_rate(t1, t2, Compounding::Continuous, Frequency::Annual, false)?
            .rate();
        let v = self
            .vol_ts
            .black_forward_variance(t1, t2, self.strike, false)?
            / (t2 - t1);

        let diffusion = self
            .dxx_map
            .mult(&Array::filled(self.mesher.layout().size(), 0.5 * v));
        self.map_t.axpyb(
            &Array::filled(1, r - q - 0.5 * v),
            &self.dx_map,
            &diffusion,
            &Array::filled(1, -r),
        );

        Ok(())
    }

    /// `cpp:115-117`: there is no mixed term on a one-dimensional mesh.
    fn apply_mixed(&self, r: &Array) -> Array {
        Array::with_size(r.size())
    }

    /// `cpp:106-113`.
    fn apply_direction(&self, direction: Size, r: &Array) -> Array {
        if direction == self.direction {
            self.map_t.apply(r)
        } else {
            Array::with_size(r.size())
        }
    }

    /// `cpp:119-126`. The timestep scales the operator and the identity keeps
    /// its unit weight, so `(dt, 1.0)` reaches
    /// [`TripleBandLinearOp::solve_splitting`] in that order; along any other
    /// direction the step is the identity.
    fn solve_splitting(&self, direction: Size, r: &Array, s: Real) -> QlResult<Array> {
        if direction == self.direction {
            self.map_t.solve_splitting(r, s, 1.0)
        } else {
            Ok(r.clone())
        }
    }

    /// `cpp:128-131`.
    fn preconditioner(&self, r: &Array, s: Real) -> QlResult<Array> {
        self.solve_splitting(self.direction, r, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::handle::Handle;
    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::methods::finitedifferences::operators::FdmLinearOpLayout;
    use crate::quotes::make_quote_handle;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::BlackConstantVol;
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::{Date, Month};
    use crate::time::daycounter::DayCounter;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::types::{Rate, Volatility};

    const DIRECTION: Size = 0;
    const R: Rate = 0.05;
    const Q: Rate = 0.02;
    const VOL: Volatility = 0.2;
    const STRIKE: Real = 100.0;
    const T1: Time = 0.5;
    const T2: Time = 0.75;
    const TOL: Real = 1e-14;

    fn mesher() -> Shared<dyn FdmMesher> {
        let layout = shared(FdmLinearOpLayout::new(vec![5]));
        shared(UniformGridMesher::new(layout, &[(4.0, 5.0)]).unwrap())
    }

    /// Flat curves, so `set_time` recovers `r`, `q` and `v = VOL * VOL`
    /// exactly. The three coefficients they produce - a drift of `0.01`, a
    /// diffusion of `0.02` and a discount of `-0.05` - are distinct and
    /// non-zero, so no term of the generator can be dropped or swapped with
    /// another without the band test seeing it.
    fn black_scholes_op(mesher: &Shared<dyn FdmMesher>) -> FdmBlackScholesOp {
        let dc = Actual365Fixed::new();
        let today = Date::new(11, Month::February, 2018);
        let process = GeneralizedBlackScholesProcess::new(
            make_quote_handle(100.0).handle(),
            flat_rate(today, Q, dc.clone()),
            flat_rate(today, R, dc.clone()),
            flat_vol(today, VOL, dc),
        );

        FdmBlackScholesOp::new(Shared::clone(mesher), &process, STRIKE, DIRECTION).unwrap()
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

    fn assert_close(actual: &Array, expected: &Array) {
        assert_eq!(actual.size(), expected.size());
        for i in 0..actual.size() {
            assert!(
                (actual[i] - expected[i]).abs() <= TOL,
                "element {i}: {} != {}",
                actual[i],
                expected[i]
            );
        }
    }

    /// The generator is `(r - q - v/2) D1 + (v/2) D2 - r I`. The expectation is
    /// built from the derivative operators the constructor is told to use, so
    /// this pins the wiring; the operators themselves are oracled in #640.
    #[test]
    fn set_time_builds_the_generator_from_the_derivative_operators() {
        let mesher = mesher();
        let mut operator = black_scholes_op(&mesher);
        operator.set_time(T1, T2).unwrap();

        let dx = first_derivative_op(DIRECTION, Shared::clone(&mesher));
        let dxx = second_derivative_op(DIRECTION, Shared::clone(&mesher));
        let u = Array::incremental(mesher.layout().size(), 1.0, 0.5);

        let v = VOL * VOL;
        let (applied_dx, applied_dxx) = (dx.apply(&u), dxx.apply(&u));
        let expected: Array = (0..u.size())
            .map(|i| (R - Q - 0.5 * v) * applied_dx[i] + 0.5 * v * applied_dxx[i] - R * u[i])
            .collect();

        assert_close(&operator.apply(&u), &expected);
    }

    /// The curves reject a step that runs backwards, and `set_time` carries
    /// that out rather than swallowing it as C++'s `void` signature must.
    #[test]
    fn set_time_rejects_a_reversed_step() {
        assert!(black_scholes_op(&mesher()).set_time(T2, T1).is_err());
    }

    /// `axpyb` overwrites the bands rather than adding to them, so stepping
    /// twice over the same interval leaves the same operator - the scheme of
    /// #657 calls `set_time` once per timestep on this one operator.
    #[test]
    fn set_time_replaces_the_previous_step() {
        let mesher = mesher();
        let mut operator = black_scholes_op(&mesher);
        let u = Array::incremental(mesher.layout().size(), 1.0, 0.5);

        operator.set_time(T1, T2).unwrap();
        let once = operator.apply(&u);
        operator.set_time(T1, T2).unwrap();

        assert_close(&operator.apply(&u), &once);
    }

    #[test]
    fn size_counts_the_splitting_directions() {
        assert_eq!(black_scholes_op(&mesher()).size(), 1);
    }

    #[test]
    fn apply_mixed_is_zero() {
        let mesher = mesher();
        let operator = black_scholes_op(&mesher);
        let u = Array::incremental(mesher.layout().size(), 1.0, 0.5);

        assert_eq!(operator.apply_mixed(&u), Array::with_size(u.size()));
    }

    #[test]
    fn apply_direction_acts_only_along_the_operator_direction() {
        let mesher = mesher();
        let mut operator = black_scholes_op(&mesher);
        operator.set_time(T1, T2).unwrap();
        let u = Array::incremental(mesher.layout().size(), 1.0, 0.5);

        assert_eq!(operator.apply_direction(DIRECTION, &u), operator.apply(&u));
        assert_eq!(
            operator.apply_direction(DIRECTION + 1, &u),
            Array::with_size(u.size())
        );
    }

    /// The implicit step solves `(I + s A) x = r`, so applying the operator and
    /// solving it back recovers the grid values; along any other direction the
    /// solve returns its argument.
    #[test]
    fn solve_splitting_inverts_the_implicit_step() {
        let mesher = mesher();
        let mut operator = black_scholes_op(&mesher);
        operator.set_time(T1, T2).unwrap();

        let u = Array::incremental(mesher.layout().size(), 2.0, -0.25);
        let s = 0.01;
        let applied = operator.apply(&u);
        let r: Array = (0..u.size()).map(|i| s * applied[i] + u[i]).collect();

        assert_close(&operator.solve_splitting(DIRECTION, &r, s).unwrap(), &u);
        assert_eq!(operator.solve_splitting(DIRECTION + 1, &r, s).unwrap(), r);
    }

    #[test]
    fn preconditioner_solves_along_the_operator_direction() {
        let mesher = mesher();
        let mut operator = black_scholes_op(&mesher);
        operator.set_time(T1, T2).unwrap();

        let r = Array::incremental(mesher.layout().size(), 1.0, 0.75);
        let s = 0.01;

        assert_eq!(
            operator.preconditioner(&r, s).unwrap(),
            operator.solve_splitting(DIRECTION, &r, s).unwrap()
        );
    }

    /// The shape the schemes of #657 hold the operator in: one shared,
    /// mutable trait object that they step through `set_time` and then read
    /// through the [`FdmLinearOp`] the boundary-condition helper takes.
    #[test]
    fn the_operator_drives_the_scheme_shapes_through_a_shared_handle() {
        let mesher = mesher();
        let handle: SharedMut<dyn FdmLinearOpComposite> = shared_mut(black_scholes_op(&mesher));
        let u = Array::incremental(mesher.layout().size(), 1.0, 0.5);

        let applied = {
            let mut composite = handle.borrow_mut();
            composite.set_time(T1, T2).unwrap();
            let linear: &mut dyn FdmLinearOp = &mut *composite;
            linear.apply(&u)
        };

        assert_eq!(handle.borrow().apply(&u), applied);
    }
}
