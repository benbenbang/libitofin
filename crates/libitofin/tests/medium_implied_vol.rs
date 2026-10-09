//! Compiled QuantLib forward-variance fixtures plus rebasing/lifecycle contracts.

use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::math::matrix::Matrix;
use libitofin::patterns::observable::{AsObservable, Observer};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::volatility::{
    BlackVarianceCurve, BlackVarianceSurface, BlackVolTermStructure, ImpliedVolTermStructure,
};
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
