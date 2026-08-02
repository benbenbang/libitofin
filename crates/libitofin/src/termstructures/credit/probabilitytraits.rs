//! Credit bootstrap traits.
//!
//! Port of the `HazardRate` trait struct in
//! `ql/termstructures/credit/probabilitytraits.hpp:115-188`.
//!
//! ## Why this is not the zero-rate trait
//!
//! A hazard rate and a zero rate are both non-negative rates stored one per
//! pillar, which makes it tempting to reuse
//! [`ZeroYield`](crate::termstructures::bootstraptraits::ZeroYield). C++ keeps
//! them as separate structs and the numbers differ in four places, so this port
//! transcribes rather than aliases: the average rate is `0.01` and not `0.05`,
//! the fresh-curve lower bracket is `QL_EPSILON` and not `-maxRate`, the
//! valid-data brackets have no sign branch, and the iteration cap is `30` and
//! not `100`.
//!
//! ## Only the core trait
//!
//! [`HazardRate`] implements
//! [`BootstrapTraits`](crate::termstructures::bootstraptraits::BootstrapTraits)
//! and deliberately does not implement
//! [`YieldBootstrapTraits`](crate::termstructures::bootstraptraits::YieldBootstrapTraits):
//! a hazard-rate node has no discount factor to hand back. The node-to-value
//! conversions this convention does need - `hazardRateImpl` and
//! `survivalProbabilityImpl` - belong to the curve
//! (`interpolatedhazardratecurve.hpp:148-165`), not to the traits struct.

use crate::termstructures::bootstraptraits::BootstrapTraits;
use crate::types::{Real, Size, Time};

/// The average and maximum hazard rate the bracket/guess formulas assume
/// (`detail::avgHazardRate` / `detail::maxHazardRate`,
/// `probabilitytraits.hpp:39-40`). These are credit-local: the yield
/// `AVG_RATE` is five times larger.
const AVG_HAZARD_RATE: Real = 0.01;
const MAX_HAZARD_RATE: Real = 1.0;

/// Hazard-rate bootstrap traits (`struct HazardRate`,
/// `probabilitytraits.hpp:115`). The curve nodes are piecewise hazard rates,
/// the reference node holds a dummy average rate, and the bracket keeps every
/// solved rate strictly positive.
pub struct HazardRate;

impl BootstrapTraits for HazardRate {
    /// The dummy value at the reference date (`initialValue`, `:130-132`).
    /// Node 0 carries no information of its own - `update_guess` overwrites it
    /// with the first solved pillar.
    fn initial_value() -> Real {
        AVG_HAZARD_RATE
    }

    /// The per-node guess (`guess`, `:135-149`).
    ///
    /// The C++ extrapolation branch reprices the partial curve itself,
    /// `c->hazardRate(c->dates()[i], true)`; the Rust trait only receives the
    /// node slices, so this returns the last solved node instead. During a
    /// bootstrap the curve's interpolation spans the solved prefix `[0, i-1]`
    /// while `times_` still holds every pillar, so that C++ call lands on
    /// `interpolation_(t, true)` extrapolating past `x_max` - which for
    /// `BackwardFlat`, the convention this curve is bootstrapped with, is the
    /// last node value exactly, and for `Linear` continues the last segment's
    /// slope instead. Either way the difference is benign by construction: the
    /// guess only seeds a bracketed solver whose converged root is independent
    /// of it, and the caller already clamps it into `[min, max]`
    /// (`iterativebootstrap.rs:182-186`).
    fn guess(i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            return data[i];
        }
        if i == 1 {
            return AVG_HAZARD_RATE;
        }
        data[i - 1]
    }

    /// The lower bracket (`minValueAfter`, `:152-162`): half the smallest node
    /// when a previous solution seeds the pass, otherwise `QL_EPSILON`.
    ///
    /// The floor is a tiny *positive* number, never negative - a hazard rate
    /// below zero is a negative default intensity. This is where the
    /// convention parts company with the yield traits, whose fresh-curve floor
    /// is `-maxRate`, and there is no sign branch on the valid-data path
    /// either: C++ halves the minimum whatever its sign.
    fn min_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            let r = data.iter().copied().fold(Real::INFINITY, Real::min);
            return r / 2.0;
        }
        Real::EPSILON
    }

    /// The upper bracket (`maxValueAfter`, `:164-176`): double the largest node
    /// when a previous solution seeds the pass, otherwise `maxHazardRate` - a
    /// value the C++ comment calls "very unlikely to be exceeded" rather than a
    /// real constraint. As with the lower bracket there is no sign branch.
    fn max_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            let r = data.iter().copied().fold(Real::NEG_INFINITY, Real::max);
            return r * 2.0;
        }
        MAX_HAZARD_RATE
    }

    /// Writes a solved rate back (`updateGuess`, `:179-185`): node `i` takes
    /// the rate, and the reference node mirrors the first pillar so the
    /// `(0, t1)` segment is not left at the dummy `initial_value`.
    fn update_guess(data: &mut [Real], value: Real, i: Size) {
        data[i] = value;
        if i == 1 {
            data[0] = value;
        }
    }

    /// The convergence-loop cap (`maxIterations`, `:187`). Credit pillars are
    /// solved in fewer passes than yield pillars, which allow `100`.
    fn max_iterations() -> Size {
        30
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termstructures::bootstraptraits::ZeroYield;

    #[test]
    fn initial_value_is_the_average_hazard_rate_not_the_average_yield() {
        assert_eq!(HazardRate::initial_value(), 0.01);
        assert_ne!(HazardRate::initial_value(), ZeroYield::initial_value());
    }

    #[test]
    fn max_iterations_is_thirty_not_the_yield_hundred() {
        assert_eq!(HazardRate::max_iterations(), 30);
        assert_ne!(HazardRate::max_iterations(), ZeroYield::max_iterations());
    }

    #[test]
    fn first_pillar_guess_is_the_average_hazard_rate() {
        let times = [0.0, 1.0];
        let data = [0.01, 0.01];
        assert_eq!(HazardRate::guess(1, &times, &data, false), 0.01);
    }

    #[test]
    fn later_pillar_guess_extends_the_previous_node_flat() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.02, 0.02, 0.01];
        assert_eq!(HazardRate::guess(2, &times, &data, false), 0.02);
    }

    #[test]
    fn valid_data_guess_reuses_the_stored_node() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.02, 0.02, 0.035];
        assert_eq!(HazardRate::guess(2, &times, &data, true), 0.035);
    }

    #[test]
    fn fresh_curve_bracket_is_strictly_positive_unlike_the_yield_bracket() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.01, 0.02, 0.01];
        let min = HazardRate::min_value_after(2, &times, &data, false);
        let max = HazardRate::max_value_after(2, &times, &data, false);
        assert_eq!(min, Real::EPSILON);
        assert!(min > 0.0);
        assert_eq!(max, 1.0);
        assert!(min < max);
        assert_ne!(min, ZeroYield::min_value_after(2, &times, &data, false));
    }

    #[test]
    fn valid_data_bracket_halves_the_minimum_and_doubles_the_maximum() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.02, 0.02, 0.05];
        assert!((HazardRate::min_value_after(2, &times, &data, true) - 0.01).abs() < 1e-15);
        assert!((HazardRate::max_value_after(2, &times, &data, true) - 0.10).abs() < 1e-15);
    }

    #[test]
    fn valid_data_bracket_has_no_sign_branch() {
        let times = [0.0, 1.0, 2.0];
        let data = [-0.02, -0.02, -0.01];
        let min = HazardRate::min_value_after(2, &times, &data, true);
        let max = HazardRate::max_value_after(2, &times, &data, true);
        assert!((min - (-0.01)).abs() < 1e-15);
        assert!((max - (-0.02)).abs() < 1e-15);
        assert_ne!(min, ZeroYield::min_value_after(2, &times, &data, true));
        assert_ne!(max, ZeroYield::max_value_after(2, &times, &data, true));
    }

    #[test]
    fn update_guess_writes_the_node_and_mirrors_the_first_pillar() {
        let mut data = [0.01, 0.01, 0.01];
        HazardRate::update_guess(&mut data, 0.03, 1);
        assert_eq!(data[1], 0.03);
        assert_eq!(data[0], 0.03);

        HazardRate::update_guess(&mut data, 0.04, 2);
        assert_eq!(data[2], 0.04);
        assert_eq!(data[0], 0.03);
    }
}
