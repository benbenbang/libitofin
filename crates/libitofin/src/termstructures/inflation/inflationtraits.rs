//! Inflation bootstrap traits.
//!
//! Port of the `ZeroInflationTraits` and `YoYInflationTraits` classes in
//! `ql/termstructures/inflation/inflationtraits.hpp:41-115,117-198`.
//!
//! ## Why this is not the zero-rate trait
//!
//! An inflation rate and a zero yield are both rates stored one per pillar,
//! which makes it tempting to alias
//! [`ZeroYield`](crate::termstructures::bootstraptraits::ZeroYield). C++ keeps
//! them as separate structs and the numbers differ in four places, so this
//! port transcribes rather than aliases: the average rate is `0.02` and not
//! `0.05`, the fresh-curve guess is that average at *every* pillar rather than
//! the previous node from the second one on, the fresh-curve bracket is
//! `+/-0.5` and not `+/-1.0`, and the iteration cap is `40` and not `100`.
//!
//! ## Where `initialDate` went
//!
//! C++'s `ZeroInflationTraits::initialDate` returns the curve's `baseDate()`
//! (`inflationtraits.hpp:46-48`), which is what puts the first bootstrap node
//! *before* the reference date. In this port that decision belongs to the
//! curve, as
//! [`PiecewiseCurve::initial_date`](crate::termstructures::iterativebootstrap::PiecewiseCurve::initial_date),
//! not to the traits: the driver reads it off the curve it is bootstrapping, so
//! a piecewise zero-inflation curve overrides that method with its base date.
//!
//! ## Only the core trait
//!
//! Both structs implement
//! [`BootstrapTraits`] and deliberately do not implement
//! [`YieldBootstrapTraits`](crate::termstructures::bootstraptraits::YieldBootstrapTraits):
//! the nodes *are* the zero-inflation rates, and there is no discount factor to
//! convert them into. C++'s `transformDirect`/`transformInverse`
//! (`inflationtraits.hpp:105-112`) are not ported either: both are the
//! identity, they exist for an unconstrained-optimization path this port does
//! not have, and the bootstrap driver never calls them.
//!
//! ## Zero against year-on-year
//!
//! [`YoYInflationTraits`] (`inflationtraits.hpp:118-191`) is transcribed
//! separately rather than derived, because C++ writes the two structs out in
//! full and they differ in exactly three places: the helper family they name,
//! the seeding of node 0, and the absence of the node-0 mirror in
//! `updateGuess`. Only the last of the three is visible here; the first two are
//! decisions this port carries on the curve rather than in the traits, and the
//! divergence is documented on each.

use crate::termstructures::bootstraptraits::BootstrapTraits;
use crate::types::{Real, Size, Time};

/// The average and maximum inflation rate the bracket/guess formulas assume
/// (`detail::avgInflation` / `detail::maxInflation`,
/// `inflationtraits.hpp:36-37`). Both are inflation-local: the yield `AVG_RATE`
/// is more than twice as large and its `MAX_RATE` is double this.
const AVG_INFLATION: Real = 0.02;
const MAX_INFLATION: Real = 0.5;

/// Zero-inflation bootstrap traits (`class ZeroInflationTraits`,
/// `inflationtraits.hpp:41`). The curve nodes are zero-coupon inflation rates,
/// the base-date node holds a dummy average rate, and the bracket is a wide
/// band that admits deflation.
pub struct ZeroInflationTraits;

impl BootstrapTraits for ZeroInflationTraits {
    /// The dummy value at the first node (`initialValue`, `:50-53`). The C++
    /// comment is explicit that it "will be overwritten during bootstrap":
    /// node 0 sits at the base date and carries no information of its own -
    /// `update_guess` overwrites it with the first solved pillar.
    fn initial_value() -> Real {
        AVG_INFLATION
    }

    /// The per-node guess (`guess`, `:56-68`): the stored node when a previous
    /// solution seeds the pass, otherwise the average inflation rate.
    ///
    /// This is where the convention parts company with every rate-storing yield
    /// trait. Those return the average only at `i == 1` and extrapolate the
    /// previous node from there on; this one has no such branch and seeds every
    /// fresh pillar at the same `0.02`. The guess only seeds a bracketed solver
    /// whose converged root is independent of it, so the difference is benign,
    /// but it is the C++ behaviour and is transcribed as such.
    fn guess(i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            return data[i];
        }
        AVG_INFLATION
    }

    /// The lower bracket (`minValueAfter`, `:71-82`): on a seeded pass, the
    /// smallest node doubled when negative and halved when positive - the sign
    /// branch widening the bracket either way - otherwise `-maxInflation`.
    ///
    /// The fresh-curve floor is negative, unlike the credit
    /// [`HazardRate`](crate::termstructures::credit::probabilitytraits::HazardRate)
    /// floor: deflation is a real state of the world, a negative default
    /// intensity is not.
    fn min_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            let r = data.iter().copied().fold(Real::INFINITY, Real::min);
            return if r < 0.0 { r * 2.0 } else { r / 2.0 };
        }
        -MAX_INFLATION
    }

    /// The upper bracket (`maxValueAfter`, `:84-97`): the largest node halved
    /// when negative and doubled when positive, otherwise `maxInflation` - a
    /// value the C++ comment calls "very unlikely to be exceeded" rather than a
    /// real constraint.
    fn max_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            let r = data.iter().copied().fold(Real::NEG_INFINITY, Real::max);
            return if r < 0.0 { r / 2.0 } else { r * 2.0 };
        }
        MAX_INFLATION
    }

    /// Writes a solved rate back (`updateGuess`, `:100-102`): node `i` takes
    /// the rate, and the base-date node mirrors the first pillar so the segment
    /// from the base date to the first pillar is not left at the dummy
    /// `initial_value`.
    fn update_guess(data: &mut [Real], value: Real, i: Size) {
        data[i] = value;
        if i == 1 {
            data[0] = value;
        }
    }

    /// The convergence-loop cap (`maxIterations`, `:114`). Inflation pillars
    /// are solved in fewer passes than yield pillars, which allow `100`.
    fn max_iterations() -> Size {
        40
    }
}

/// Year-on-year inflation bootstrap traits (`class YoYInflationTraits`,
/// `inflationtraits.hpp:118`). The curve nodes are year-on-year inflation
/// rates; the guesses, the brackets and the iteration cap are the
/// zero-inflation ones to the digit.
pub struct YoYInflationTraits;

impl BootstrapTraits for YoYInflationTraits {
    /// The average inflation rate (`initialValue`, `:129-131`, where C++ reads
    /// the *curve's* base rate off the term-structure pointer it is handed).
    ///
    /// This constant is therefore never the seeding path for a year-on-year
    /// curve. That decision belongs to the curve here, as
    /// [`PiecewiseCurve::initial_value`](crate::termstructures::iterativebootstrap::PiecewiseCurve::initial_value),
    /// which
    /// [`PiecewiseYoYInflationCurve`](super::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve)
    /// overrides with its base rate; the driver reads it off the curve exactly
    /// as C++'s reads `Traits::initialValue(t)`. What is left here is the value
    /// C++ would return for a curve carrying no base rate, and it is kept
    /// rather than made unreachable because the C++ static is a total function.
    fn initial_value() -> Real {
        AVG_INFLATION
    }

    /// The per-node guess (`guess`, `:135-146`): byte for byte the
    /// zero-inflation one - the stored node on a seeded pass, otherwise the
    /// average inflation rate at every pillar.
    fn guess(i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            return data[i];
        }
        AVG_INFLATION
    }

    /// The lower bracket (`minValueAfter`, `:149-160`), identical to the
    /// zero-inflation one: the smallest node doubled when negative and halved
    /// when positive, otherwise `-maxInflation`.
    fn min_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            let r = data.iter().copied().fold(Real::INFINITY, Real::min);
            return if r < 0.0 { r * 2.0 } else { r / 2.0 };
        }
        -MAX_INFLATION
    }

    /// The upper bracket (`maxValueAfter`, `:161-173`), identical to the
    /// zero-inflation one: the largest node halved when negative and doubled
    /// when positive, otherwise `maxInflation`.
    fn max_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            let r = data.iter().copied().fold(Real::NEG_INFINITY, Real::max);
            return if r < 0.0 { r / 2.0 } else { r * 2.0 };
        }
        MAX_INFLATION
    }

    /// Writes a solved rate back (`updateGuess`, `:175-179`): node `i` takes
    /// the rate and **nothing else moves**.
    ///
    /// This is the one difference from
    /// [`ZeroInflationTraits::update_guess`] visible in this file, and it is
    /// load-bearing rather than incidental. Node 0 of a year-on-year curve
    /// carries the curve's own base rate, seeded through the curve's
    /// `initial_value` hook; mirroring the first solved pillar onto it - which
    /// is exactly what the zero convention does, its node 0 holding only a
    /// dummy - would overwrite a real quoted figure with an interpolated one.
    fn update_guess(data: &mut [Real], value: Real, i: Size) {
        data[i] = value;
    }

    /// The convergence-loop cap (`maxIterations`, `:190`), the same `40`.
    fn max_iterations() -> Size {
        40
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termstructures::bootstraptraits::ZeroYield;

    #[test]
    fn initial_value_is_the_average_inflation_rate_not_the_average_yield() {
        assert_eq!(ZeroInflationTraits::initial_value(), 0.02);
        assert_ne!(
            ZeroInflationTraits::initial_value(),
            ZeroYield::initial_value()
        );
    }

    #[test]
    fn max_iterations_is_forty_not_the_yield_hundred() {
        assert_eq!(ZeroInflationTraits::max_iterations(), 40);
        assert_ne!(
            ZeroInflationTraits::max_iterations(),
            ZeroYield::max_iterations()
        );
    }

    #[test]
    fn every_fresh_pillar_guesses_the_average_rate_where_the_yield_trait_extrapolates() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.02, 0.03, 0.02];

        assert_eq!(ZeroInflationTraits::guess(1, &times, &data, false), 0.02);
        assert_eq!(ZeroInflationTraits::guess(2, &times, &data, false), 0.02);
        assert_ne!(
            ZeroInflationTraits::guess(2, &times, &data, false),
            ZeroYield::guess(2, &times, &data, false)
        );
    }

    #[test]
    fn valid_data_guess_reuses_the_stored_node() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.02, 0.02, 0.035];
        assert_eq!(ZeroInflationTraits::guess(2, &times, &data, true), 0.035);
    }

    #[test]
    fn fresh_curve_bracket_is_the_half_percent_band_around_zero() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.02, 0.03, 0.02];
        let min = ZeroInflationTraits::min_value_after(2, &times, &data, false);
        let max = ZeroInflationTraits::max_value_after(2, &times, &data, false);

        assert_eq!(min, -0.5);
        assert_eq!(max, 0.5);
        assert!(min < 0.0, "the bracket must admit deflation");
        assert_ne!(min, ZeroYield::min_value_after(2, &times, &data, false));
        assert_ne!(max, ZeroYield::max_value_after(2, &times, &data, false));
    }

    #[test]
    fn valid_data_bracket_halves_a_positive_minimum_and_doubles_a_positive_maximum() {
        let times = [0.0, 1.0, 2.0];
        let data = [0.02, 0.02, 0.05];
        assert!(
            (ZeroInflationTraits::min_value_after(2, &times, &data, true) - 0.01).abs() < 1e-15
        );
        assert!(
            (ZeroInflationTraits::max_value_after(2, &times, &data, true) - 0.10).abs() < 1e-15
        );
    }

    #[test]
    fn valid_data_bracket_flips_the_branches_on_negative_nodes() {
        let times = [0.0, 1.0, 2.0];
        let data = [-0.02, -0.02, -0.01];
        let min = ZeroInflationTraits::min_value_after(2, &times, &data, true);
        let max = ZeroInflationTraits::max_value_after(2, &times, &data, true);

        assert!((min - (-0.04)).abs() < 1e-15);
        assert!((max - (-0.005)).abs() < 1e-15);
        assert!(min < max);
    }

    #[test]
    fn update_guess_writes_the_node_and_mirrors_the_base_date_node() {
        let mut data = [0.02, 0.02, 0.02];
        ZeroInflationTraits::update_guess(&mut data, 0.03, 1);
        assert_eq!(data[1], 0.03);
        assert_eq!(data[0], 0.03);

        ZeroInflationTraits::update_guess(&mut data, 0.04, 2);
        assert_eq!(data[2], 0.04);
        assert_eq!(data[0], 0.03);
    }

    /// The crux of the year-on-year convention: the first solved pillar is
    /// *not* mirrored onto node 0, where the zero convention mirrors it.
    #[test]
    fn the_year_on_year_update_guess_leaves_the_base_node_alone() {
        let mut yoy = [0.029, 0.02, 0.02];
        YoYInflationTraits::update_guess(&mut yoy, 0.031, 1);
        assert_eq!(yoy[1], 0.031);
        assert_eq!(yoy[0], 0.029, "the base-rate node must survive pillar 1");

        let mut zero = [0.029, 0.02, 0.02];
        ZeroInflationTraits::update_guess(&mut zero, 0.031, 1);
        assert_ne!(zero[0], yoy[0]);
    }

    /// The traits' own `initial_value` stays the C++ constant. It is not the
    /// seeding path - the curve's `initial_value` hook is, and returns the base
    /// rate - so this pins the constant that would otherwise drift towards the
    /// behaviour it is not responsible for. Its counterpart is the piecewise
    /// curve's non-degenerate base-node assertion, which fails if the seeding
    /// ever comes from here.
    #[test]
    fn the_year_on_year_traits_seed_is_the_average_rate_not_a_curve_base_rate() {
        assert_eq!(YoYInflationTraits::initial_value(), 0.02);
        assert_eq!(
            YoYInflationTraits::initial_value(),
            ZeroInflationTraits::initial_value()
        );
    }

    /// Everything but `update_guess` is the zero convention to the digit
    /// (`inflationtraits.hpp:135-173,190` against `:56-97,114`).
    #[test]
    fn the_year_on_year_guesses_brackets_and_cap_match_the_zero_convention() {
        let times = [0.0, 1.0, 2.0];
        for data in [[0.02, 0.03, 0.05], [-0.02, -0.02, -0.01]] {
            for valid in [false, true] {
                assert_eq!(
                    YoYInflationTraits::guess(2, &times, &data, valid),
                    ZeroInflationTraits::guess(2, &times, &data, valid)
                );
                assert_eq!(
                    YoYInflationTraits::min_value_after(2, &times, &data, valid),
                    ZeroInflationTraits::min_value_after(2, &times, &data, valid)
                );
                assert_eq!(
                    YoYInflationTraits::max_value_after(2, &times, &data, valid),
                    ZeroInflationTraits::max_value_after(2, &times, &data, valid)
                );
            }
        }
        assert_eq!(YoYInflationTraits::max_iterations(), 40);
    }
}
