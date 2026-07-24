//! Bootstrap traits and the mutable node plumbing they drive.
//!
//! Port of `ql/termstructures/yield/bootstraptraits.hpp` (the `Discount`
//! traits) together with the curve-side node storage the bootstrap mutates.
//!
//! ## The mutation decision
//!
//! In C++ `IterativeBootstrap` is a *friend* of `PiecewiseYieldCurve` and
//! writes straight into `ts_->data_`, `ts_->interpolation_` and `ts_->dates_`
//! (`iterativebootstrap.hpp:296-316`). Rust has no friendship and no
//! inheritance, so the decision this port makes is: the node data lives in a
//! [`CurveData`] holder the curve keeps behind a `RefCell`, and the bootstrap
//! mutates it through this type's methods. The bootstrap borrows the cell
//! mutably to write one node and rebuild the interpolation over the solved
//! prefix, then *drops that borrow* before asking a helper to reprice - the
//! helper reads the same curve back (through its weak handle), so the two
//! borrows must never overlap. That discipline is what makes a `RefCell`
//! sufficient here instead of a second ownership scheme.
//!
//! ## Scope
//!
//! The `Discount` and `ZeroYield` traits are ported. `SimpleZeroYield`
//! (`bootstraptraits.hpp:312`) is a documented deferral: it adds a `-1/t`
//! rate floor and exp/log transforms this port does not need yet.

use crate::errors::QlResult;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::time::date::Date;
use crate::types::{DiscountFactor, Real, Size, Time};
use crate::{fail, require};

/// The average and maximum instantaneous rate the `Discount` bracket/guess
/// formulas assume (`detail::avgRate` / `detail::maxRate`,
/// `bootstraptraits.hpp:39-40`).
const AVG_RATE: Real = 0.05;
const MAX_RATE: Real = 1.0;

/// Curve-shape traits that drive the bootstrap's initial value, per-node guess
/// and search bracket (C++'s `Discount`/`ZeroYield`/... trait structs).
///
/// The formulas here are load-bearing: they are what makes the per-node root
/// search converge, so they transcribe the C++ statics exactly rather than
/// being reinvented. Every method reads the node `times` and the partially
/// solved `data`, matching the C++ `c->times()` / `c->data()` access.
pub trait BootstrapTraits {
    /// The value at the reference-date node (`Traits::initialValue`).
    fn initial_value() -> Real;

    /// The initial guess for node `i` (`Traits::guess`). `valid_data` is set
    /// when a previous curve state is being reused as the starting point.
    fn guess(i: Size, times: &[Time], data: &[Real], valid_data: bool) -> Real;

    /// The lower bracket bound for node `i` (`Traits::minValueAfter`).
    fn min_value_after(i: Size, times: &[Time], data: &[Real], valid_data: bool) -> Real;

    /// The upper bracket bound for node `i` (`Traits::maxValueAfter`).
    fn max_value_after(i: Size, times: &[Time], data: &[Real], valid_data: bool) -> Real;

    /// Writes a solved value back into the node vector (`Traits::updateGuess`).
    fn update_guess(data: &mut [Real], value: Real, i: Size);

    /// The convergence-loop iteration cap (`Traits::maxIterations`).
    fn max_iterations() -> Size;

    /// Converts an interpolated node at time `t` into a discount factor,
    /// applying this convention's node meaning (a discount factor for
    /// `Discount`, `exp(-z*t)` for a zero rate, `exp(-primitive)` for an
    /// instantaneous forward). This is the seam that lets one `CurveData`
    /// holder serve every convention: the holder stays convention-agnostic and
    /// the traits interpret its nodes.
    ///
    /// Everything a conversion needs - the last solved node time, its value,
    /// the derivative and the antiderivative - is carried by the
    /// [`Interpolation`] the bootstrap rebuilt over the solved prefix
    /// `[0, upto]`, so `interpolation.x_max()` is that last node's time and
    /// `value(x_max)` its value; past it every convention extends the last
    /// instantaneous forward flat.
    fn discount_from_nodes<I: Interpolation>(
        interpolation: &I,
        t: Time,
    ) -> QlResult<DiscountFactor>;
}

/// Discount-factor bootstrap traits (`struct Discount`,
/// `bootstraptraits.hpp:44`). The curve nodes are discount factors, the
/// reference node is 1.0, and the bracket keeps every factor positive and
/// bounded by a `MAX_RATE` instantaneous forward over each segment.
pub struct Discount;

impl BootstrapTraits for Discount {
    fn initial_value() -> Real {
        1.0
    }

    fn guess(i: Size, times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            return data[i];
        }
        if i == 1 {
            return 1.0 / (1.0 + AVG_RATE * times[1]);
        }
        // flat instantaneous-forward extrapolation from the previous node
        let r = -data[i - 1].ln() / times[i - 1];
        (-r * times[i]).exp()
    }

    fn min_value_after(i: Size, times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            let min = data.iter().copied().fold(Real::INFINITY, Real::min);
            return min / 2.0;
        }
        let dt = times[i] - times[i - 1];
        data[i - 1] * (-MAX_RATE * dt).exp()
    }

    fn max_value_after(i: Size, times: &[Time], data: &[Real], _valid_data: bool) -> Real {
        let dt = times[i] - times[i - 1];
        data[i - 1] * (MAX_RATE * dt).exp()
    }

    fn update_guess(data: &mut [Real], value: Real, i: Size) {
        data[i] = value;
    }

    fn max_iterations() -> Size {
        100
    }

    /// The nodes are already discount factors, so the value is the discount
    /// factor directly (in range), and past the last solved node the last
    /// instantaneous forward continues flat (the port of
    /// `InterpolatedDiscountCurve::discountImpl`).
    fn discount_from_nodes<I: Interpolation>(
        interpolation: &I,
        t: Time,
    ) -> QlResult<DiscountFactor> {
        let t_max = interpolation.x_max();
        if t <= t_max {
            return interpolation.value(t);
        }
        let d_max = interpolation.value(t_max)?;
        let inst_fwd_max = -interpolation.derivative(t_max)? / d_max;
        Ok(d_max * (-inst_fwd_max * (t - t_max)).exp())
    }
}

/// The per-node guess the rate-storing conventions share (`ZeroYield`/
/// `ForwardRate` `guess`, `bootstraptraits.hpp:147-163`/`:246-256`). The C++
/// extrapolation branch reprices the partial curve's own `zeroRate`/
/// `forwardRate`; the Rust trait only receives the node slices, so this returns
/// the last solved node instead. That is benign by construction: the guess only
/// seeds a bracketed solver whose converged root is independent of it, and the
/// caller already clamps the guess into `[min, max]`
/// (`iterativebootstrap.rs:182-186`).
fn rate_guess(i: Size, data: &[Real], valid_data: bool) -> Real {
    if valid_data {
        return data[i];
    }
    if i == 1 {
        return AVG_RATE;
    }
    data[i - 1]
}

/// The lower bracket the rate-storing conventions share (`minValueAfter`,
/// `:167-179`/`:266-278`): half a positive minimum, double a negative one, or
/// an unconstrained `-maxRate` on a fresh curve.
fn rate_min_value_after(data: &[Real], valid_data: bool) -> Real {
    if valid_data {
        let r = data.iter().copied().fold(Real::INFINITY, Real::min);
        return if r < 0.0 { r * 2.0 } else { r / 2.0 };
    }
    -MAX_RATE
}

/// The upper bracket the rate-storing conventions share (`maxValueAfter`,
/// `:180-193`/`:279-292`): double a positive maximum, half a negative one, or
/// an unconstrained `+maxRate` on a fresh curve.
fn rate_max_value_after(data: &[Real], valid_data: bool) -> Real {
    if valid_data {
        let r = data.iter().copied().fold(Real::NEG_INFINITY, Real::max);
        return if r < 0.0 { r / 2.0 } else { r * 2.0 };
    }
    MAX_RATE
}

/// The solved-value write the rate-storing conventions share (`updateGuess`,
/// `:207-214`/`:306-313`): node `i` takes the rate, and the reference node
/// mirrors the first pillar so the `(0, t1)` segment is not left at
/// `initial_value`.
fn rate_update_guess(data: &mut [Real], value: Real, i: Size) {
    data[i] = value;
    if i == 1 {
        data[0] = value;
    }
}

/// Zero-yield bootstrap traits (`struct ZeroYield`, `bootstraptraits.hpp:127`).
/// The curve nodes are continuously compounded zero rates; the reference node
/// starts at the average rate and the bracket keeps each rate within
/// `[-maxRate, maxRate]` on a fresh pass.
pub struct ZeroYield;

impl BootstrapTraits for ZeroYield {
    fn initial_value() -> Real {
        AVG_RATE
    }

    fn guess(i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        rate_guess(i, data, valid_data)
    }

    fn min_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        rate_min_value_after(data, valid_data)
    }

    fn max_value_after(_i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        rate_max_value_after(data, valid_data)
    }

    fn update_guess(data: &mut [Real], value: Real, i: Size) {
        rate_update_guess(data, value, i);
    }

    fn max_iterations() -> Size {
        100
    }

    /// The node is a zero rate `z(t)`, so the discount factor is `exp(-z*t)`.
    /// In range the rate is the interpolated value; past the last solved node
    /// the last instantaneous forward continues flat, mirroring
    /// `InterpolatedZeroCurve::zeroYieldImpl` (`zerocurve.rs:164-182`).
    fn discount_from_nodes<I: Interpolation>(
        interpolation: &I,
        t: Time,
    ) -> QlResult<DiscountFactor> {
        let t_max = interpolation.x_max();
        let z = if t <= t_max {
            interpolation.value(t)?
        } else {
            let z_max = interpolation.value(t_max)?;
            let inst_fwd_max = z_max + t_max * interpolation.derivative(t_max)?;
            (z_max * t_max + inst_fwd_max * (t - t_max)) / t
        };
        Ok((-z * t).exp())
    }
}

/// Mutable node storage of a piecewise curve: the pillar dates and times, the
/// solved values (discount factors for `Discount`), and the interpolation
/// rebuilt over the solved prefix.
///
/// This is the curve-side half of the mutation decision. The bootstrap holds
/// it through a `RefCell` on the curve and drives it a node at a time; the
/// curve's `discount` reads it back. During a bootstrap the interpolation only
/// spans the solved prefix `[0, upto]`, so [`discount`](Self::discount)
/// extrapolates past the last solved node with a flat instantaneous forward,
/// exactly as `InterpolatedDiscountCurve::discountImpl` does past its last
/// node - which is also why a helper for pillar `i` (whose latest relevant
/// date is at most `times[i]`) only ever reads in-range values.
pub struct CurveData<I: Interpolator> {
    dates: Vec<Date>,
    times: Vec<Time>,
    data: Vec<Real>,
    interpolation: Option<I::Output>,
    max_date: Option<Date>,
    valid: bool,
}

impl<I: Interpolator> Default for CurveData<I> {
    fn default() -> Self {
        CurveData::new()
    }
}

impl<I: Interpolator> CurveData<I> {
    /// An empty, un-bootstrapped holder (the curve is built cheap; the nodes
    /// are filled lazily on the first calculation).
    pub fn new() -> CurveData<I> {
        CurveData {
            dates: Vec::new(),
            times: Vec::new(),
            data: Vec::new(),
            interpolation: None,
            max_date: None,
            valid: false,
        }
    }

    /// Whether a previous bootstrap left a usable solution to seed the next one
    /// (C++'s `validCurve_`).
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Marks the current node values as a usable solution.
    pub fn set_valid(&mut self, valid: bool) {
        self.valid = valid;
    }

    /// Installs the pillar dates and times for a bootstrap pass. The value
    /// vector is left untouched so a valid previous solution can seed the next
    /// pass; [`reset_data`](Self::reset_data) clears it when it cannot.
    pub fn set_pillars(&mut self, dates: Vec<Date>, times: Vec<Time>) {
        self.dates = dates;
        self.times = times;
        self.interpolation = None;
    }

    /// Resets every node to `initial_value` (C++'s `data_ =
    /// vector(alive+1, initialValue)`), discarding any prior solution.
    pub fn reset_data(&mut self, initial_value: Real, len: usize) {
        self.data = vec![initial_value; len];
        self.valid = false;
    }

    /// The pillar dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The pillar times.
    pub fn times(&self) -> &[Time] {
        &self.times
    }

    /// The node values.
    pub fn data(&self) -> &[Real] {
        &self.data
    }

    /// Mutable access to the node values, for the traits' `update_guess`.
    pub fn data_mut(&mut self) -> &mut [Real] {
        &mut self.data
    }

    /// The curve's maximum date (the latest relevant date over all helpers).
    pub fn max_date(&self) -> Option<Date> {
        self.max_date
    }

    /// Records the curve's maximum date.
    pub fn set_max_date(&mut self, date: Date) {
        self.max_date = Some(date);
    }

    /// Whether the nodes have been laid out (a bootstrap has at least started).
    pub fn is_initialized(&self) -> bool {
        !self.times.is_empty()
    }

    /// The (date, value) nodes.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.data.iter().copied())
            .collect()
    }

    /// Rebuilds the interpolation over the solved prefix `[0, upto]`
    /// (C++'s `interpolateWithoutUpdate(..., times.begin()+upto+1, ...)`).
    pub fn rebuild(&mut self, interpolator: &I, upto: usize) -> QlResult<()> {
        self.interpolation =
            Some(interpolator.interpolate(&self.times[..=upto], &self.data[..=upto])?);
        Ok(())
    }

    /// The interpolation rebuilt over the solved prefix, for a trait's
    /// [`discount_from_nodes`](BootstrapTraits::discount_from_nodes) to read the
    /// node value, derivative or antiderivative at a time. Errors before the
    /// bootstrap has laid one down.
    pub fn interpolation(&self) -> QlResult<&I::Output> {
        match self.interpolation.as_ref() {
            Some(interpolation) => Ok(interpolation),
            None => fail!("curve not bootstrapped: no interpolation available"),
        }
    }

    /// Asserts the holder has been bootstrapped, for inspectors that must not
    /// hand back an empty curve.
    pub fn require_initialized(&self) -> QlResult<()> {
        require!(self.is_initialized(), "curve not bootstrapped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::interpolations::loglinear::LogLinear;

    #[test]
    fn discount_initial_value_is_one() {
        assert_eq!(Discount::initial_value(), 1.0);
    }

    #[test]
    fn first_pillar_guess_uses_the_average_rate() {
        let times = [0.0, 0.5];
        let data = [1.0, 1.0];
        let guess = Discount::guess(1, &times, &data, false);
        assert!((guess - 1.0 / (1.0 + AVG_RATE * 0.5)).abs() < 1e-15);
    }

    #[test]
    fn later_pillar_guess_extrapolates_the_previous_forward_flat() {
        // node 1 at t=0.5 with df 0.98 -> r = -ln(0.98)/0.5; node 2 at t=1.0
        // guesses exp(-r*1.0).
        let times = [0.0, 0.5, 1.0];
        let data = [1.0, 0.98, 1.0];
        let r = -0.98_f64.ln() / 0.5;
        let guess = Discount::guess(2, &times, &data, false);
        assert!((guess - (-r * 1.0).exp()).abs() < 1e-15);
    }

    #[test]
    fn valid_data_guess_reuses_the_stored_node() {
        let times = [0.0, 0.5, 1.0];
        let data = [1.0, 0.98, 0.95];
        assert_eq!(Discount::guess(2, &times, &data, true), 0.95);
    }

    #[test]
    fn bracket_bounds_a_max_rate_forward_around_the_previous_node() {
        let times = [0.0, 0.5, 1.0];
        let data = [1.0, 0.98, 1.0];
        let dt = 0.5;
        let min = Discount::min_value_after(2, &times, &data, false);
        let max = Discount::max_value_after(2, &times, &data, false);
        assert!((min - 0.98 * (-MAX_RATE * dt).exp()).abs() < 1e-15);
        assert!((max - 0.98 * (MAX_RATE * dt).exp()).abs() < 1e-15);
        assert!(min < max);
    }

    #[test]
    fn valid_data_min_halves_the_smallest_node() {
        let times = [0.0, 0.5, 1.0];
        let data = [1.0, 0.98, 0.90];
        let min = Discount::min_value_after(2, &times, &data, true);
        assert!((min - 0.90 / 2.0).abs() < 1e-15);
    }

    #[test]
    fn update_guess_writes_the_node() {
        let mut data = [1.0, 0.98, 1.0];
        Discount::update_guess(&mut data, 0.95, 2);
        assert_eq!(data[2], 0.95);
    }

    #[test]
    fn curve_data_discount_interpolates_in_range_and_extends_flat_beyond() {
        let mut cd = CurveData::<LogLinear>::new();
        cd.set_pillars(
            vec![Date::null(), Date::null(), Date::null()],
            vec![0.0, 1.0, 2.0],
        );
        cd.reset_data(1.0, 3);
        cd.data_mut()[1] = 0.95;
        cd.data_mut()[2] = 0.88;
        cd.rebuild(&LogLinear, 2).unwrap();

        // in range: geometric interpolation of log-linear discounts
        let mid = Discount::discount_from_nodes(cd.interpolation().unwrap(), 1.5).unwrap();
        assert!((mid - (0.95_f64 * 0.88).sqrt()).abs() < 1e-12);

        // past the last node: flat instantaneous forward continues
        let last_forward = (0.95_f64 / 0.88).ln();
        let beyond = Discount::discount_from_nodes(cd.interpolation().unwrap(), 3.0).unwrap();
        assert!((beyond - 0.88 * (-last_forward * 1.0).exp()).abs() < 1e-12);
    }

    #[test]
    fn curve_data_interpolation_before_bootstrap_is_an_error() {
        let cd = CurveData::<LogLinear>::new();
        assert!(cd.interpolation().is_err());
        assert!(cd.require_initialized().is_err());
    }
}
