use libitofin::handle::Handle;
use libitofin::instruments::NullPayoff;
use libitofin::interestrate::Compounding;
use libitofin::math::array::Array;
use libitofin::models::model::CalibratedModelHolder;
use libitofin::models::shortrate::{HullWhite, OneFactorAffineModel};
use libitofin::payoff::Payoff;
use libitofin::processes::BatesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn curve(rate: f64) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        Date::new(9, Month::October, 2026),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}

#[test]
fn null_payoff_names_and_failure_match_quantlib() {
    let dummy: &dyn Payoff = &NullPayoff;
    assert_eq!(dummy.name(), "Null");
    assert_eq!(dummy.description(), "Null");
    for price in [0.0, 100.0, f64::NAN] {
        let error = std::panic::catch_unwind(|| NullPayoff.value(price)).unwrap_err();
        assert_eq!(error.downcast_ref::<&str>(), Some(&"dummy payoff given"));
    }
}

#[test]
fn hull_white_sigma_tracks_parameters_without_shadowing_affine_a() {
    let model = HullWhite::new(curve(0.03), 0.1, 0.01).unwrap();
    assert_eq!(model.borrow().sigma(), 0.01);
    let original_affine_a = model.borrow().a(0.0, 2.0);
    assert!(original_affine_a.is_finite());
    model
        .borrow_mut()
        .set_params(&Array::from([0.2, 0.025]))
        .unwrap();
    assert_eq!(model.borrow().sigma(), 0.025);
    assert!(model.borrow().a(0.0, 2.0).is_finite());
}

#[test]
fn bates_jump_mean_matches_quantlib_law_and_preserves_small_means() {
    let spot = Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>);
    for (nu, delta, expected) in [
        (-0.1, 0.2, -0.07688365361336424),
        (1e-16, 0.0, 1e-16),
        (0.0, 0.0, 0.0),
    ] {
        let process = BatesProcess::new(
            curve(0.03),
            curve(0.01),
            spot.clone(),
            0.04,
            1.0,
            0.04,
            0.2,
            -0.5,
            0.1,
            nu,
            delta,
        )
        .unwrap();
        assert!((process.m() - expected).abs() <= 1e-14 * expected.abs().max(1e-16));
        assert_eq!(process.nu(), nu);
        assert_eq!(process.delta(), delta);
        assert_eq!(process.lambda(), 0.1);
    }
}
