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

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::Cell;

    use crate::instruments::PlainVanillaPayoff;
    use crate::methods::finitedifferences::meshers::UniformGridMesher;
    use crate::methods::finitedifferences::operators::FdmLinearOpLayout;
    use crate::option::OptionType;
    use crate::shared::shared;

    const DIM: [Size; 2] = [5, 3];
    const BOUNDARIES: [(Real, Real); 2] = [(80.0, 120.0), (0.0, 1.0)];

    /// Two dimensions, so that a grid point is not its own averaging
    /// coordinate: the layout holds 15 points over 5 coordinates along
    /// direction 0.
    fn mesher() -> Shared<dyn FdmMesher> {
        let layout = shared(FdmLinearOpLayout::new(DIM.to_vec()));
        shared(UniformGridMesher::new(layout, &BOUNDARIES).unwrap())
    }

    fn call(strike: Real) -> Shared<dyn Payoff> {
        shared(PlainVanillaPayoff::new(OptionType::Call, strike))
    }

    fn calculator(strike: Real) -> FdmCellAveragingInnerValue {
        FdmCellAveragingInnerValue::new(call(strike), mesher(), 0)
    }

    /// A payoff that is a spike at one point and negligible everywhere else.
    ///
    /// The negligible value drives the accuracy heuristic (`cpp:96-97`) below
    /// machine epsilon, which `SimpsonIntegral::new` rejects; the spike then
    /// makes the fallback visible, because an integral that did run would
    /// average the spike away.
    struct SpikePayoff {
        location: Real,
    }

    impl Payoff for SpikePayoff {
        fn name(&self) -> String {
            "Spike".to_string()
        }

        fn description(&self) -> String {
            "Spike".to_string()
        }

        fn value(&self, price: Real) -> Real {
            if (price - self.location).abs() < 1e-9 {
                42.0
            } else {
                1e-13
            }
        }
    }

    /// A plain-vanilla payoff that counts how often it is evaluated.
    struct CountingPayoff {
        inner: PlainVanillaPayoff,
        evaluations: Cell<usize>,
    }

    impl Payoff for CountingPayoff {
        fn name(&self) -> String {
            self.inner.name()
        }

        fn description(&self) -> String {
            self.inner.description()
        }

        fn value(&self, price: Real) -> Real {
            self.evaluations.set(self.evaluations.get() + 1);
            self.inner.value(price)
        }
    }

    /// Smoke test, not a QuantLib oracle: batch 2 has none, and the numbers are
    /// validated downstream by `testCrankNicolsonWithDamping`
    /// (`fdmlinearop.cpp:1291`). The property is `cpp:85-86`, which returns the
    /// grid-point value verbatim at the outermost coordinates.
    ///
    /// The strike sits on the lowest grid point, so the cell around it is half
    /// in the money: an average taken over it would be strictly positive where
    /// the grid-point value is zero.
    #[test]
    fn the_outermost_cells_take_the_grid_point_value() {
        let calculator = calculator(80.0);
        let mesher = mesher();

        let mut boundary_points = 0;
        for position in mesher.layout().iter() {
            let coord = position.coordinates()[0];
            if coord == 0 || coord == DIM[0] - 1 {
                boundary_points += 1;
                assert_eq!(
                    calculator.avg_inner_value(&position, 0.0),
                    calculator.inner_value(&position, 0.0)
                );
            }
        }
        assert_eq!(boundary_points, 2 * DIM[1]);
    }

    /// The cache holds one value per coordinate along the averaging direction,
    /// not one per grid point (`cpp:66-78`): points differing only in the other
    /// coordinate share it.
    #[test]
    fn the_average_depends_only_on_the_averaging_coordinate() {
        let calculator = calculator(100.0);
        let mesher = mesher();

        let mut by_coordinate = vec![None; DIM[0]];
        for position in mesher.layout().iter() {
            let value = calculator.avg_inner_value(&position, 0.0);
            match by_coordinate[position.coordinates()[0]] {
                None => by_coordinate[position.coordinates()[0]] = Some(value),
                Some(first) => assert_eq!(value, first),
            }
        }
        assert!(by_coordinate.iter().all(Option::is_some));
    }

    /// Simpson is exact on a linear integrand, so a cell that lies entirely in
    /// the money averages to its midpoint - which is the grid point itself on a
    /// uniform grid. That pins the `/(b-a)` normalisation (`cpp:98`).
    #[test]
    fn a_cell_over_a_linear_payoff_averages_to_its_grid_point() {
        let calculator = calculator(84.0);
        let mesher = mesher();

        for position in mesher.layout().iter() {
            let coord = position.coordinates()[0];
            if coord == 0 || coord == DIM[0] - 1 {
                continue;
            }
            let expected = calculator.inner_value(&position, 0.0);
            let average = calculator.avg_inner_value(&position, 0.0);
            assert!(
                (average - expected).abs() <= 1e-13 * expected,
                "coordinate {coord}: {average} vs {expected}"
            );
        }
    }

    /// A cell straddling the strike averages strictly above its grid point: the
    /// integral picks up the kink the grid-point value misses. This is what the
    /// cell averaging exists for, and it fails if the fallback is taken.
    #[test]
    fn a_cell_straddling_the_strike_averages_above_its_grid_point() {
        let calculator = calculator(100.0);
        let mesher = mesher();
        let at_the_money = mesher.layout().iter().find(|p| p.coordinates()[0] == 2);
        let at_the_money = at_the_money.unwrap();

        assert_eq!(calculator.inner_value(&at_the_money, 0.0), 0.0);
        assert!(calculator.avg_inner_value(&at_the_money, 0.0) > 0.0);
    }

    /// `FdmLogInnerValue` reads the grid as log-underlying (`cpp:114`).
    #[test]
    fn the_log_calculator_evaluates_the_payoff_at_the_exponential() {
        let payoff = PlainVanillaPayoff::new(OptionType::Call, 100.0);
        let layout = shared(FdmLinearOpLayout::new(vec![7]));
        let mesher: Shared<dyn FdmMesher> =
            shared(UniformGridMesher::new(layout, &[(4.0, 5.0)]).unwrap());
        let calculator = fdm_log_inner_value(shared(payoff), Shared::clone(&mesher), 0);

        for position in mesher.layout().iter() {
            let x = mesher.location(&position, 0);
            assert_eq!(
                calculator.inner_value(&position, 0.0),
                payoff.value(x.exp())
            );
        }
    }

    /// An accuracy the integrator rejects falls back to the grid-point value
    /// rather than propagating the error, standing in for the C++
    /// `catch (Error&)` (`cpp:99-103`).
    #[test]
    fn an_unusable_accuracy_falls_back_to_the_grid_point_value() {
        let mesher = mesher();
        let interior = mesher.layout().iter().find(|p| p.coordinates()[0] == 1);
        let interior = interior.unwrap();
        let location = mesher.location(&interior, 0);
        let calculator =
            FdmCellAveragingInnerValue::new(shared(SpikePayoff { location }), mesher, 0);

        assert_eq!(calculator.inner_value(&interior, 0.0), 42.0);
        assert_eq!(calculator.avg_inner_value(&interior, 0.0), 42.0);
    }

    /// The cache answers every call after the first (`cpp:64-65`).
    #[test]
    fn the_average_is_computed_once_per_coordinate() {
        let payoff = shared(CountingPayoff {
            inner: PlainVanillaPayoff::new(OptionType::Call, 100.0),
            evaluations: Cell::new(0),
        });
        let mesher = mesher();
        let calculator = FdmCellAveragingInnerValue::new(
            Shared::clone(&payoff) as Shared<dyn Payoff>,
            Shared::clone(&mesher),
            0,
        );

        let first: Vec<Real> = mesher
            .layout()
            .iter()
            .map(|position| calculator.avg_inner_value(&position, 0.0))
            .collect();
        let evaluations = payoff.evaluations.get();
        assert!(evaluations > 0);

        let second: Vec<Real> = mesher
            .layout()
            .iter()
            .map(|position| calculator.avg_inner_value(&position, 0.0))
            .collect();
        assert_eq!(second, first);
        assert_eq!(payoff.evaluations.get(), evaluations);
    }
}
