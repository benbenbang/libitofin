//! The terminal payoff the backward solver seeds the grid with.
//!
//! Port of `ql/methods/finitedifferences/utilities/fdminnervaluecalculator.hpp:43,52,73`
//! and its `.cpp`.
//!
//! Two of the header's calculators are out of this port's scope and are left
//! for follow-up work: `FdmLogBasketInnerValue` (`hpp:80`), which needs the
//! unported `BasketPayoff`, and `FdmZeroInnerValue` (`hpp:93`).

use std::cell::RefCell;

use crate::math::integrals::Integrator;
use crate::math::integrals::simpson::SimpsonIntegral;
use crate::methods::finitedifferences::meshers::FdmMesher;
use crate::methods::finitedifferences::operators::FdmLinearOpIterator;
use crate::payoff::Payoff;
use crate::shared::Shared;
use crate::types::{Real, Size, Time};

/// The value of an instrument at a grid point, before any rollback.
///
/// C++ declares both methods non-const so the cell-averaging implementation can
/// fill its cache (`hpp:47-48`); the Rust trait takes `&self` and leaves the
/// caching to interior mutability, because consumers hold the calculator as a
/// [`Shared`] and a `Shared` yields no `&mut`.
pub trait FdmInnerValueCalculator {
    /// The payoff at the grid point itself.
    fn inner_value(&self, iter: &FdmLinearOpIterator, t: Time) -> Real;

    /// The payoff averaged over the cell around the grid point.
    fn avg_inner_value(&self, iter: &FdmLinearOpIterator, t: Time) -> Real;
}

/// How a grid coordinate maps to the underlying the payoff is written on.
///
/// C++ takes an arbitrary `std::function<Real(Real)>` (`hpp:57`), but the whole
/// tree constructs only these two: the identity (`fdcevvanillaengine.cpp:116`,
/// `fdsabrvanillaengine.cpp:103`) and `exp` (`FdmLogInnerValue`, `cpp:114`). The
/// enum keeps the type inspectable; a third mapping is one variant away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridMapping {
    /// The grid holds the underlying directly.
    Identity,
    /// The grid holds the log of the underlying.
    Exp,
}

impl GridMapping {
    fn apply(self, x: Real) -> Real {
        match self {
            GridMapping::Identity => x,
            GridMapping::Exp => x.exp(),
        }
    }
}

/// A payoff sampled on the grid, averaged over each cell along one direction.
///
/// The average is cached per coordinate along `direction`, not per grid point:
/// the payoff depends on that one coordinate, so every point sharing it shares
/// the value (`cpp:66-78`). The cache is filled in full on the first
/// [`avg_inner_value`](Self::avg_inner_value) call and answers every later one,
/// which carries over a C++ quirk: the `t` of that first call is the `t` the
/// whole cache is built with, and every subsequent `t` is ignored (`cpp:64-79`).
/// Neither payoff mapping here uses `t`, so the two are equivalent today; the
/// port keeps the C++ behaviour rather than silently making the cache
/// time-aware.
pub struct FdmCellAveragingInnerValue {
    payoff: Shared<dyn Payoff>,
    mesher: Shared<dyn FdmMesher>,
    direction: Size,
    grid_mapping: GridMapping,
    avg_inner_values: RefCell<Vec<Real>>,
}

impl FdmCellAveragingInnerValue {
    /// A calculator reading `payoff` off `direction` of `mesher` directly
    /// (C++'s default `gridMapping`, `hpp:57`).
    pub fn new(payoff: Shared<dyn Payoff>, mesher: Shared<dyn FdmMesher>, direction: Size) -> Self {
        Self::with_grid_mapping(payoff, mesher, direction, GridMapping::Identity)
    }

    /// A calculator mapping each grid coordinate through `grid_mapping` before
    /// evaluating `payoff`.
    pub fn with_grid_mapping(
        payoff: Shared<dyn Payoff>,
        mesher: Shared<dyn FdmMesher>,
        direction: Size,
        grid_mapping: GridMapping,
    ) -> Self {
        FdmCellAveragingInnerValue {
            payoff,
            mesher,
            direction,
            grid_mapping,
            avg_inner_values: RefCell::new(Vec::new()),
        }
    }

    /// The cell average at `iter` (`cpp:81-106`).
    ///
    /// The two outermost coordinates have no full cell around them and take the
    /// grid-point value instead (`cpp:85-86`). Elsewhere the cell spans half a
    /// spacing either side, and the average is a Simpson integral over it
    /// divided by its width. Both ways the integral can fail - the accuracy
    /// heuristic can land at or below machine epsilon, and eight iterations
    /// leave a narrow convergence window - stand in for the C++ `catch (Error&)`
    /// (`cpp:99-103`) and fall back to the grid-point value.
    #[allow(clippy::float_cmp)]
    fn avg_inner_value_calc(&self, iter: &FdmLinearOpIterator, t: Time) -> Real {
        let dim = self.mesher.layout().dim()[self.direction];
        let coord = iter.coordinates()[self.direction];
        if coord == 0 || coord == dim - 1 {
            return self.inner_value(iter, t);
        }

        let loc = self.mesher.location(iter, self.direction);
        let a = loc - self.mesher.dminus(iter, self.direction) / 2.0;
        let b = loc + self.mesher.dplus(iter, self.direction) / 2.0;

        let f = |x: Real| self.payoff.value(self.grid_mapping.apply(x));
        let accuracy = if f(a) != 0.0 || f(b) != 0.0 {
            (f(a) + f(b)) * 5e-5
        } else {
            1e-4
        };

        SimpsonIntegral::new(accuracy, 8)
            .and_then(|integral| integral.integrate(f, a, b))
            .map(|integral| integral / (b - a))
            .unwrap_or_else(|_| self.inner_value(iter, t))
    }
}

impl FdmInnerValueCalculator for FdmCellAveragingInnerValue {
    fn inner_value(&self, iter: &FdmLinearOpIterator, _t: Time) -> Real {
        let loc = self.mesher.location(iter, self.direction);
        self.payoff.value(self.grid_mapping.apply(loc))
    }

    fn avg_inner_value(&self, iter: &FdmLinearOpIterator, t: Time) -> Real {
        let uninitialized = self.avg_inner_values.borrow().is_empty();
        if uninitialized {
            let dim = self.mesher.layout().dim()[self.direction];
            let mut values = vec![0.0; dim];
            let mut initialized = vec![false; dim];
            for position in self.mesher.layout().iter() {
                let coord = position.coordinates()[self.direction];
                if !initialized[coord] {
                    initialized[coord] = true;
                    values[coord] = self.avg_inner_value_calc(&position, t);
                }
            }
            *self.avg_inner_values.borrow_mut() = values;
        }

        self.avg_inner_values.borrow()[iter.coordinates()[self.direction]]
    }
}

/// A calculator over a log-space grid: the payoff is evaluated at `exp(x)`
/// (`cpp:108-114`).
///
/// C++ makes this a subclass whose only content is that mapping (`hpp:73`), so
/// the port is a constructor function over the one concrete type. Consumers
/// hold a [`Shared<dyn FdmInnerValueCalculator>`](FdmInnerValueCalculator), so
/// nothing needs the name in a type position.
pub fn fdm_log_inner_value(
    payoff: Shared<dyn Payoff>,
    mesher: Shared<dyn FdmMesher>,
    direction: Size,
) -> FdmCellAveragingInnerValue {
    FdmCellAveragingInnerValue::with_grid_mapping(payoff, mesher, direction, GridMapping::Exp)
}
