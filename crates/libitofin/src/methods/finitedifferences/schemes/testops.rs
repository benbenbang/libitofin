//! The fixtures the scheme tests drive.
//!
//! Two operators, because neither alone is enough. [`ScaledComposite`] is
//! diagonal, so a scheme over it has a closed form and its three methods carry
//! three different coefficients - which is what tells `apply` apart from
//! `apply_direction`. [`black_scholes_op`] is the real operator of #656, which
//! pins that a scheme composes with one; there `apply` and
//! `apply_direction(0, .)` are literally the same function
//! (`fdmblackscholesop.rs:91` and `:141`), so it cannot tell them apart.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::math::array::Array;
use crate::methods::finitedifferences::BoundaryCondition;
use crate::methods::finitedifferences::meshers::{FdmMesher, UniformGridMesher};
use crate::methods::finitedifferences::operators::{
    FdmBlackScholesOp, FdmLinearOp, FdmLinearOpComposite, FdmLinearOpLayout,
};
use crate::methods::finitedifferences::utilities::FdmBoundaryConditionSet;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::quotes::make_quote_handle;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::{Date, Month};
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Size, Time, Volatility};

/// The coefficient [`ScaledComposite`] scales the whole operator by.
pub const WHOLE: Real = 0.7;

/// The number of grid points [`mesher`] lays out.
pub const GRID: Size = 5;

const DIRECTION: Size = 0;
const R: Rate = 0.05;
const Q: Rate = 0.02;
const VOL: Volatility = 0.2;
const STRIKE: Real = 100.0;
const TOL: Real = 1e-13;

/// A diagonal composite whose splitting inverts exactly.
///
/// `apply` scales by [`WHOLE`], direction `d` scales by its own coefficient,
/// and `solve_splitting(d, r, s)` returns `r / (1 + s c_d)`, the exact solution
/// of `(I + s A_d) x = r`. Give it coefficients that differ from [`WHOLE`] and
/// from each other and every one of the three shows up separately in the
/// result of a step.
///
/// It therefore breaks the invariant a real composite keeps - `apply` is not
/// the sum of the directions and the mixed term - and does so deliberately:
/// an operator whose parts agree is exactly one that cannot say which part a
/// scheme reached for.
pub struct ScaledComposite {
    coefficients: Vec<Real>,
    /// The arguments the last `set_time` was given, `None` before the first.
    pub last_set_time: Option<(Time, Time)>,
}

impl ScaledComposite {
    fn new(coefficients: &[Real]) -> Self {
        ScaledComposite {
            coefficients: coefficients.to_vec(),
            last_set_time: None,
        }
    }
}

impl FdmLinearOp for ScaledComposite {
    fn apply(&self, r: &Array) -> Array {
        WHOLE * r
    }
}

impl FdmLinearOpComposite for ScaledComposite {
    fn size(&self) -> Size {
        self.coefficients.len()
    }

    fn set_time(&mut self, t1: Time, t2: Time) -> QlResult<()> {
        self.last_set_time = Some((t1, t2));
        Ok(())
    }

    fn apply_mixed(&self, r: &Array) -> Array {
        Array::with_size(r.size())
    }

    fn apply_direction(&self, direction: Size, r: &Array) -> Array {
        self.coefficients[direction] * r
    }

    fn solve_splitting(&self, direction: Size, r: &Array, s: Real) -> QlResult<Array> {
        Ok(r / (1.0 + s * self.coefficients[direction]))
    }

    fn preconditioner(&self, r: &Array, s: Real) -> QlResult<Array> {
        self.solve_splitting(0, r, s)
    }
}

/// A [`ScaledComposite`] with one direction per coefficient.
pub fn scaled_composite(coefficients: &[Real]) -> SharedMut<ScaledComposite> {
    shared_mut(ScaledComposite::new(coefficients))
}

/// A boundary condition that records which calls reach it and leaves the grid
/// values untouched, so the numbers of a step stay those of the empty set.
pub struct CallLog {
    log: Shared<RefCell<Vec<String>>>,
}

impl BoundaryCondition for CallLog {
    fn apply_before_applying(&self, _op: &mut dyn FdmLinearOp) {
        self.log.borrow_mut().push("before_applying".to_string());
    }

    fn apply_after_applying(&self, _a: &mut Array) {
        self.log.borrow_mut().push("after_applying".to_string());
    }

    fn apply_before_solving(&self, _op: &mut dyn FdmLinearOp, _rhs: &mut Array) {
        self.log.borrow_mut().push("before_solving".to_string());
    }

    fn apply_after_solving(&self, _a: &mut Array) {
        self.log.borrow_mut().push("after_solving".to_string());
    }

    fn set_time(&self, t: Time) {
        self.log.borrow_mut().push(format!("set_time:{t}"));
    }
}

/// A one-condition set and the log it writes to.
pub fn call_log() -> (Shared<RefCell<Vec<String>>>, FdmBoundaryConditionSet) {
    let log = shared(RefCell::new(Vec::new()));
    let bc_set: FdmBoundaryConditionSet = vec![shared(CallLog {
        log: Shared::clone(&log),
    })];

    (log, bc_set)
}

/// A uniform grid of [`GRID`] points.
pub fn mesher() -> Shared<dyn FdmMesher> {
    let layout = shared(FdmLinearOpLayout::new(vec![GRID]));
    shared(UniformGridMesher::new(layout, &[(4.0, 5.0)]).unwrap())
}

/// The Black-Scholes generator of #656 over flat curves.
pub fn black_scholes_op(mesher: &Shared<dyn FdmMesher>) -> FdmBlackScholesOp {
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

fn flat_vol(reference: Date, vol: Volatility, dc: DayCounter) -> Handle<dyn BlackVolTermStructure> {
    Handle::new(shared(BlackConstantVol::new(reference, None, vol, dc))
        as Shared<dyn BlackVolTermStructure>)
}

/// The grid values every scheme test steps.
///
/// Quadratic, and that is load-bearing on the Black-Scholes arm: the
/// second-derivative operator annihilates a linear probe, which leaves the
/// diffusion coefficient multiplying zero and hides any error in it. That
/// blind spot is what #656's own tests were fixed for.
pub fn probe(n: Size) -> Array {
    (0..n)
        .map(|i| {
            let i = i as Real;
            1.0 + 0.5 * i + 0.05 * i * i
        })
        .collect()
}

/// Asserts two arrays agree element by element.
pub fn assert_close(actual: &Array, expected: &Array) {
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
