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
