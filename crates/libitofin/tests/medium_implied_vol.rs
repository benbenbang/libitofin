//! Compiled QuantLib forward-variance fixtures plus rebasing/lifecycle contracts.

use std::cell::Cell;

use libitofin::errors::QlResult;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::math::matrix::Matrix;
use libitofin::patterns::observable::{AsObservable, Observable, Observer};
use libitofin::quotes::{SimpleQuote, make_quote_handle};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{
    BlackConstantVol, BlackVarianceCurve, BlackVarianceSurface, BlackVolTermStructure,
    ImpliedVolTermStructure, VolatilityTermStructure,
};
use libitofin::termstructures::{TermStructure, TermStructureBase};
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;

fn reference() -> Date {
    Date::new(15, Month::June, 2026)
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual:.17e}, expected={expected:.17e}"
    );
}

fn variance_curve(reference: Date, day_counter: DayCounter) -> Shared<dyn BlackVolTermStructure> {
    shared(
        BlackVarianceCurve::new(
            reference,
            &[reference + 180, reference + 360, reference + 720],
            &[0.18, 0.22, 0.28],
            day_counter,
            true,
        )
        .unwrap(),
    )
}

#[derive(Default)]
struct Counter(usize);

impl Observer for Counter {
    fn update(&mut self) {
        self.0 += 1;
    }
}

fn subscribe(curve: &impl AsObservable) -> SharedMut<Counter> {
    let counter = shared_mut(Counter::default());
    curve
        .observable()
        .register_observer(&(counter.clone() as SharedMut<dyn Observer>));
    counter
}

#[test]
fn nonflat_curve_matches_compiled_quantlib_forward_variance_fixtures() {
    let original = variance_curve(reference(), Actual360::new());
    let implied = ImpliedVolTermStructure::new(Handle::new(original), reference() + 180);
    let rows = [
        (0.0, 0.0, 0.25377155080851027),
        (1e-6, 6.440000000287882e-8, 0.25377155081466246),
        (0.25, 0.016099999999999996, 0.2537715508089904),
        (0.5, 0.0322, 0.2537715508089904),
        (1.0, 0.0864, 0.29393876913398137),
        (1.5, 0.14060000000000003, 0.3061590000854676),
        (2.0, 0.17980000000000004, 0.299833287011299),
    ];
    for (t, variance, vol) in rows {
        close(
            implied.black_variance(t, 100.0, true).unwrap(),
            variance,
            1e-15,
        );
        close(implied.black_vol(t, 100.0, true).unwrap(), vol, 2e-11);
    }
    let maturity = reference() + 450;
    let t = implied.time_from_reference(maturity).unwrap();
    assert_eq!(t, 0.75);
    close(
        implied.black_variance_date(maturity, 100.0, false).unwrap(),
        implied.black_variance(t, 100.0, false).unwrap(),
        1e-15,
    );
    assert_eq!(implied.max_date(), reference() + 720);
    assert_eq!(implied.max_time().unwrap(), 1.5);
}

#[test]
fn relinking_changes_day_counter_shift_and_values_without_moving_implied_reference() {
    let link = RelinkableHandle::new(variance_curve(reference(), Actual360::new()));
    let implied = ImpliedVolTermStructure::new(link.handle(), reference() + 180);
    let counter = subscribe(&implied);
    let replacement: Shared<dyn BlackVolTermStructure> = shared(
        BlackVarianceCurve::new(
            reference() + 30,
            &[reference() + 210, reference() + 390, reference() + 750],
            &[0.21, 0.25, 0.31],
            Actual365Fixed::new(),
            true,
        )
        .unwrap(),
    );
    link.link_to(replacement);
    assert_eq!(counter.borrow().0, 1);
    assert_eq!(implied.reference_date().unwrap(), reference() + 180);
    assert_eq!(implied.day_counter().unwrap().name(), "Actual/365 (Fixed)");
    assert_eq!(implied.max_date(), reference() + 750);
    close(implied.max_time().unwrap(), 570.0 / 365.0, 1e-15);
    for (t, variance, vol) in [
        (0.0, 0.0, 0.21000000000007857),
        (0.25, 0.017200342465753422, 0.26230015223597125),
        (0.75, 0.0661736301369863, 0.29703788790205715),
    ] {
        close(
            implied.black_variance(t, 100.0, false).unwrap(),
            variance,
            1e-15,
        );
        close(implied.black_vol(t, 100.0, false).unwrap(), vol, 2e-12);
    }
}

fn surface() -> Shared<dyn BlackVolTermStructure> {
    let mut matrix = Matrix::with_size(2, 3);
    for (i, row) in [[0.2, 0.24, 0.3], [0.3, 0.34, 0.4]].into_iter().enumerate() {
        for (j, value) in row.into_iter().enumerate() {
            matrix[(i, j)] = value;
        }
    }
    shared(
        BlackVarianceSurface::new(
            reference(),
            Some(Target::new()),
            &[reference() + 180, reference() + 360, reference() + 720],
            vec![80.0, 120.0],
            &matrix,
            Actual360::new(),
        )
        .unwrap(),
    )
}

#[test]
fn strike_is_preserved_numerically_but_smile_rebasing_is_not_financially_recommended() {
    let implied = ImpliedVolTermStructure::new(Handle::new(surface()), reference() + 180);
    for (t, strike, variance, vol) in [
        (0.0, 80.0, 0.0, 0.274226184015354),
        (0.0, 100.0, 0.0, 0.32893768406683555),
        (0.0, 120.0, 0.0, 0.37576588456009524),
        (0.25, 80.0, 0.018799999999999997, 0.27422618401604176),
        (0.25, 100.0, 0.027050000000000005, 0.3289376840679706),
        (0.25, 120.0, 0.03530000000000001, 0.37576588456111876),
        (0.75, 80.0, 0.0682, 0.3015515434106304),
        (0.75, 100.0, 0.09495, 0.35580893749314396),
        (0.75, 120.0, 0.12170000000000002, 0.4028233690672212),
    ] {
        close(
            implied.black_variance(t, strike, false).unwrap(),
            variance,
            1e-15,
        );
        close(implied.black_vol(t, strike, false).unwrap(), vol, 2e-12);
    }
}

#[test]
fn own_range_and_strike_checks_apply_before_internal_forced_extrapolation() {
    let original = surface();
    original.enable_extrapolation();
    let link = RelinkableHandle::new(original);
    let implied = ImpliedVolTermStructure::new(link.handle(), reference() + 180);
    assert!(!implied.allows_extrapolation());
    assert!(implied.calendar().is_none());
    assert!(implied.settlement_days().is_err());
    assert_eq!(
        implied.business_day_convention(),
        BusinessDayConvention::Following
    );
    assert_eq!(implied.min_strike(), 80.0);
    assert_eq!(implied.max_strike(), 120.0);
    assert!(implied.black_variance(2.0, 100.0, false).is_err());
    assert!(implied.black_variance(0.25, 79.0, false).is_err());
    assert!(
        implied
            .black_variance_date(reference() + 721, 100.0, false)
            .is_err()
    );
    assert!(implied.black_variance(2.0, 79.0, true).is_ok());
    implied.enable_extrapolation();
    link.link_to(surface());
    assert!(implied.allows_extrapolation());
    assert!(implied.black_variance(2.0, 79.0, false).is_ok());
    implied.disable_extrapolation();
    for t in [-1.0, f64::INFINITY, f64::NAN] {
        assert!(implied.black_vol(t, 100.0, true).is_err());
        assert!(implied.black_variance(t, 100.0, true).is_err());
    }
    for strike in [f64::INFINITY, f64::NAN] {
        assert!(implied.black_variance(0.25, strike, true).is_err());
    }
    assert!(
        implied
            .black_vol_date(reference() + 179, 100.0, true)
            .is_err()
    );
}

struct PolynomialVariance {
    base: TermStructureBase,
    coefficient: Cell<f64>,
}

impl AsObservable for PolynomialVariance {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for PolynomialVariance {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn max_date(&self) -> Date {
        reference() + 1000
    }
}

impl VolatilityTermStructure for PolynomialVariance {
    fn business_day_convention(&self) -> BusinessDayConvention {
        BusinessDayConvention::ModifiedFollowing
    }
    fn min_strike(&self) -> f64 {
        50.0
    }
    fn max_strike(&self) -> f64 {
        150.0
    }
}

impl BlackVolTermStructure for PolynomialVariance {
    fn black_vol_impl(&self, t: f64, _strike: f64) -> QlResult<f64> {
        Ok((self.coefficient.get() + 0.01 * t).sqrt())
    }
    fn black_variance_impl(&self, t: f64, _strike: f64) -> QlResult<f64> {
        Ok(t * (self.coefficient.get() + 0.01 * t))
    }
}

#[test]
fn zero_time_uses_epsilon_but_tiny_positive_times_are_not_clamped() {
    let original: Shared<dyn BlackVolTermStructure> = shared(PolynomialVariance {
        base: TermStructureBase::with_reference_date(reference(), None, Some(Actual360::new())),
        coefficient: Cell::new(0.04),
    });
    let implied = ImpliedVolTermStructure::new(Handle::new(original), reference() + 180);
    close(
        implied.black_vol(0.0, 100.0, false).unwrap(),
        0.0500001_f64.sqrt(),
        2e-12,
    );
    close(
        implied.black_vol(1e-6, 100.0, false).unwrap(),
        0.05000001_f64.sqrt(),
        2e-11,
    );
    assert_eq!(implied.black_variance(0.0, 100.0, false).unwrap(), 0.0);
}

#[test]
fn moving_underlying_reference_and_updates_are_recomputed_without_cache() {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let original = shared(PolynomialVariance {
        base: TermStructureBase::moving(0, Target::new(), Some(Actual360::new()), settings.clone()),
        coefficient: Cell::new(0.04),
    });
    let implied = ImpliedVolTermStructure::new(
        Handle::new(original.clone() as Shared<dyn BlackVolTermStructure>),
        reference() + 180,
    );
    let counter = subscribe(&implied);
    close(
        implied.black_variance(0.5, 100.0, false).unwrap(),
        0.0275,
        1e-15,
    );
    settings.set_evaluation_date(reference() + 30);
    assert_eq!(counter.borrow().0, 1);
    assert_eq!(implied.reference_date().unwrap(), reference() + 180);
    assert!(implied.settlement_days().is_err());
    assert_eq!(
        implied.business_day_convention(),
        BusinessDayConvention::Following
    );
    let shift = original.time_from_reference(reference() + 180).unwrap();
    close(
        implied.black_variance(0.5, 100.0, false).unwrap(),
        0.02 + 0.01 * shift + 0.0025,
        1e-15,
    );
    original.coefficient.set(0.09);
    original.observable().notify_observers();
    assert_eq!(counter.borrow().0, 2);
    close(
        implied.black_variance(0.5, 100.0, false).unwrap(),
        0.045 + 0.01 * shift + 0.0025,
        1e-15,
    );
}

#[test]
fn empty_reset_null_reference_and_missing_day_counter_fail_without_panic() {
    let link: RelinkableHandle<dyn BlackVolTermStructure> = RelinkableHandle::empty();
    let implied = ImpliedVolTermStructure::new(link.handle(), reference() + 180);
    assert!(implied.day_counter().is_none());
    assert_eq!(implied.max_date(), Date::null());
    assert!(implied.black_vol(0.0, 100.0, true).is_err());
    let counter = subscribe(&implied);
    link.link_to(variance_curve(reference(), Actual360::new()));
    assert_eq!(counter.borrow().0, 1);
    assert!(implied.black_vol(0.5, 100.0, false).is_ok());
    link.reset();
    assert_eq!(counter.borrow().0, 2);
    assert!(implied.black_variance(0.5, 100.0, true).is_err());
    let no_counter: Shared<dyn BlackVolTermStructure> = shared(PolynomialVariance {
        base: TermStructureBase::with_reference_date(reference(), None, None),
        coefficient: Cell::new(0.04),
    });
    link.link_to(no_counter);
    assert!(implied.black_variance(0.5, 100.0, true).is_err());
    let null = ImpliedVolTermStructure::new(link.handle(), Date::null());
    assert!(null.reference_date().is_err());
    assert!(null.black_variance(0.5, 100.0, true).is_err());
}

#[test]
fn quote_notifications_upstream_registration_and_drops_preserve_weak_ownership() {
    let quote = make_quote_handle(0.2);
    let original = shared(BlackConstantVol::with_quote(
        reference(),
        None,
        quote.handle(),
        Actual360::new(),
    ));
    let weak = Shared::downgrade(&original);
    let link = RelinkableHandle::new(original.clone() as Shared<dyn BlackVolTermStructure>);
    let implied = ImpliedVolTermStructure::new(link.handle(), reference() + 180);
    let updater = implied.updater();
    let weak_updater = SharedMut::downgrade(&updater);
    drop(updater);
    let counter = subscribe(&implied);
    let upstream = shared_mut(Counter::default());
    implied.register_upstream(&(upstream.clone() as SharedMut<dyn Observer>));
    quote.link_to(shared(SimpleQuote::new(0.31)));
    assert_eq!(counter.borrow().0, 1);
    assert_eq!(upstream.borrow().0, 1);
    close(implied.black_vol(0.5, 100.0, false).unwrap(), 0.31, 1e-15);
    drop(original);
    drop(link);
    assert!(weak.upgrade().is_some());
    drop(implied);
    assert!(weak.upgrade().is_none());
    assert!(weak_updater.upgrade().is_none());
    quote.link_to(shared(SimpleQuote::new(0.4)));
    assert_eq!(counter.borrow().0, 1);
}

#[test]
fn decreasing_invalid_and_overflowing_original_variance_propagate_errors() {
    let decreasing: Shared<dyn BlackVolTermStructure> = shared(
        BlackVarianceCurve::new(
            reference(),
            &[reference() + 360, reference() + 720],
            &[0.4, 0.1],
            Actual360::new(),
            false,
        )
        .unwrap(),
    );
    let implied = ImpliedVolTermStructure::new(Handle::new(decreasing), reference() + 360);
    assert!(implied.black_variance(1.0, 100.0, false).is_err());
    assert!(implied.black_vol(0.0, 100.0, false).is_err());
    for coefficient in [f64::NAN, f64::INFINITY, -0.04] {
        let invalid: Shared<dyn BlackVolTermStructure> = shared(PolynomialVariance {
            base: TermStructureBase::with_reference_date(reference(), None, Some(Actual360::new())),
            coefficient: Cell::new(coefficient),
        });
        let implied = ImpliedVolTermStructure::new(Handle::new(invalid), reference() + 180);
        assert!(implied.black_variance(0.25, 100.0, true).is_err());
        assert!(implied.black_vol(0.25, 100.0, true).is_err());
    }
    let overflowing: Shared<dyn BlackVolTermStructure> = shared(PolynomialVariance {
        base: TermStructureBase::with_reference_date(reference(), None, Some(Actual360::new())),
        coefficient: Cell::new(0.04),
    });
    let implied = ImpliedVolTermStructure::new(Handle::new(overflowing), reference() + 180);
    assert!(implied.black_variance(f64::MAX, 100.0, true).is_err());
}
