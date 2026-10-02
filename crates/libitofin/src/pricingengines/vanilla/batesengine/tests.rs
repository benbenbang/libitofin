pub(super) use super::super::analytichestonengine::AnalyticHestonEngine;
pub(super) use super::*;
pub(super) use crate::exercise::{AmericanExercise, EuropeanExercise, Exercise};
pub(super) use crate::handle::{Handle, RelinkableHandle};
pub(super) use crate::instrument::Instrument;
pub(super) use crate::instruments::{
    CashOrNothingPayoff, PlainVanillaPayoff, StrikedTypePayoff, VanillaOption,
};
pub(super) use crate::interestrate::Compounding;
pub(super) use crate::math::array::Array;
pub(super) use crate::models::HestonModel;
pub(super) use crate::quotes::{Quote, SimpleQuote};
pub(super) use crate::settings::Settings;
pub(super) use crate::shared::{Shared, shared, shared_mut};
pub(super) use crate::termstructures::yields::FlatForward;
pub(super) use crate::termstructures::yieldtermstructure::YieldTermStructure;
pub(super) use crate::time::date::{Date, Month};
pub(super) use crate::time::daycounters::actual360::Actual360;
pub(super) use crate::time::frequency::Frequency;
pub(super) use crate::types::Real;

pub(super) fn reference() -> Date {
    Date::new(2, Month::October, 2026)
}
pub(super) fn quote(value: Real) -> Shared<SimpleQuote> {
    shared(SimpleQuote::new(value))
}
pub(super) fn quote_handle(value: &Shared<SimpleQuote>) -> Handle<dyn Quote> {
    Handle::new(value.clone() as Shared<dyn Quote>)
}
fn curve(value: &Shared<SimpleQuote>) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::new(
        reference(),
        quote_handle(value),
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
pub(super) fn parameters() -> Array {
    Array::from([0.05, 1.5, 0.3, -0.6, 0.04, -0.1, 0.2, 0.4])
}

pub(super) struct Market {
    pub(super) spot: Shared<SimpleQuote>,
    risk_free: Shared<SimpleQuote>,
    dividend: Shared<SimpleQuote>,
    pub(super) risk_handle: RelinkableHandle<dyn YieldTermStructure>,
    pub(super) model: SharedMut<BatesModel>,
}
impl Market {
    pub(super) fn new(params: &Array) -> Self {
        let spot = quote(100.0);
        let risk_free = quote(0.03);
        let dividend = quote(0.01);
        let risk_handle = RelinkableHandle::new(curve(&risk_free));
        let process = shared(
            BatesProcess::new(
                risk_handle.handle(),
                Handle::new(curve(&dividend)),
                quote_handle(&spot),
                params[4],
                params[1],
                params[0],
                params[2],
                params[3],
                params[7],
                params[5],
                params[6],
            )
            .unwrap(),
        );
        Self {
            spot,
            risk_free,
            dividend,
            risk_handle,
            model: BatesModel::new(process).unwrap(),
        }
    }
    pub(super) fn option(&self, kind: OptionType, strike: Real) -> VanillaOption {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference());
        let payoff = shared(PlainVanillaPayoff::new(kind, strike)) as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(EuropeanExercise::new(reference() + 360)) as Shared<dyn Exercise>;
        let mut option = VanillaOption::new(payoff, exercise, settings);
        option.base_mut().set_pricing_engine(shared_mut(
            BatesEngine::new(self.model.clone(), 160).unwrap(),
        ) as SharedMut<dyn PricingEngine>);
        option
    }
}

#[test]
fn all_eight_parameters_round_trip_and_set_params_rebuilds_process() {
    let market = Market::new(&parameters());
    let model = market.model.borrow();
    assert_eq!(model.calibrated_model().params(), parameters());
    let process = model.process();
    assert_eq!(
        [
            process.theta(),
            process.kappa(),
            process.sigma(),
            process.rho(),
            process.v0(),
            process.nu(),
            process.delta(),
            process.lambda()
        ],
        [0.05, 1.5, 0.3, -0.6, 0.04, -0.1, 0.2, 0.4]
    );
    assert_eq!(process.size(), 2);
    assert_eq!(process.factors(), 4);
    assert_eq!(
        process.initial_values().unwrap(),
        Array::from([100.0, 0.04])
    );
    assert_eq!(process.time(&(reference() + 360)).unwrap(), 1.0);
    drop(model);
    let updated = Array::from([0.06, 1.7, 0.35, -0.7, 0.07, 0.1, 0.0, 0.0]);
    market.model.borrow_mut().set_params(&updated).unwrap();
    let model = market.model.borrow();
    let current = model.process();
    assert!(!Shared::ptr_eq(&current, &process));
    assert_eq!(
        [
            model.theta(),
            model.kappa(),
            model.sigma(),
            model.rho(),
            model.v0(),
            model.nu(),
            model.delta(),
            model.lambda()
        ],
        [0.06, 1.7, 0.35, -0.7, 0.07, 0.1, 0.0, 0.0]
    );
    assert_eq!(current.v0(), 0.07);
}

#[test]
fn invalid_parameter_writes_are_atomic_and_all_domains_checked() {
    let market = Market::new(&parameters());
    let original_process = market.model.borrow().process();
    for index in 0..8 {
        for invalid in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            let mut params = parameters();
            params[index] = invalid;
            assert!(market.model.borrow_mut().set_params(&params).is_err());
        }
    }
    for (index, invalid) in [
        (0, 0.0),
        (1, -1.0),
        (2, 0.0),
        (3, -1.01),
        (3, 1.01),
        (4, 0.0),
        (6, -0.01),
        (7, -0.01),
        (5, 1000.0),
        (5, -1000.0),
        (6, 1e300),
        (7, 1e308),
    ] {
        let mut params = parameters();
        params[index] = invalid;
        if index == 7 && invalid > 0.0 {
            params[5] = 2.0;
        }
        assert!(
            market.model.borrow_mut().set_params(&params).is_err(),
            "index {index}: {invalid}"
        );
    }
    for length in [0, 7, 9] {
        assert!(
            market
                .model
                .borrow_mut()
                .set_params(&Array::with_size(length))
                .is_err()
        );
    }
    let model = market.model.borrow();
    assert_eq!(model.calibrated_model().params(), parameters());
    assert!(Shared::ptr_eq(&original_process, &model.process()));
}

#[test]
fn analytic_carrier_rejects_every_generic_gaussian_operation() {
    let market = Market::new(&parameters());
    let process = market.model.borrow().process();
    let state = Array::from([100.0, 0.04]);
    let increments = Array::from([0.1, 0.2, 0.3, 0.4]);
    assert!(process.drift(0.0, &state).is_err());
    assert!(process.diffusion(0.0, &state).is_err());
    assert!(process.apply(&state, &increments).is_err());
    assert!(process.expectation(0.0, &state, 1.0).is_err());
    assert!(process.std_deviation(0.0, &state, 1.0).is_err());
    assert!(process.covariance(0.0, &state, 1.0).is_err());
    assert!(process.evolve(0.0, &state, 1.0, &increments).is_err());
}

#[test]
fn put_call_parity_and_unavailable_greeks() {
    let market = Market::new(&parameters());
    let mut call = market.option(OptionType::Call, 105.0);
    let mut put = market.option(OptionType::Put, 105.0);
    let difference = call.npv().unwrap() - put.npv().unwrap();
    let expected = 100.0 * (-0.01_f64).exp() - 105.0 * (-0.03_f64).exp();
    assert!((difference - expected).abs() < 1e-12);
    assert!(call.delta().is_err());
    assert!(call.gamma().is_err());
    assert!(call.vega().is_err());
    assert!(call.theta().is_err());
    assert!(call.rho().is_err());
    assert!(call.dividend_rho().is_err());
}

#[test]
fn zero_jump_intensity_matches_existing_heston_engine() {
    let mut params = parameters();
    params[7] = 0.0;
    let market = Market::new(&params);
    let process = market.model.borrow().process().heston_process();
    let heston = HestonModel::new(process).unwrap();
    for kind in [OptionType::Call, OptionType::Put] {
        for strike in [80.0, 100.0, 120.0] {
            let mut bates = market.option(kind, strike);
            let mut baseline = market.option(kind, strike);
            baseline.base_mut().set_pricing_engine(shared_mut(
                AnalyticHestonEngine::new(heston.clone(), 160).unwrap(),
            ) as SharedMut<dyn PricingEngine>);
            assert!((bates.npv().unwrap() - baseline.npv().unwrap()).abs() < 1e-10);
        }
    }
}

#[test]
fn quote_curve_relink_and_parameter_notifications_reprice_cached_option() {
    let market = Market::new(&parameters());
    let mut call = market.option(OptionType::Call, 100.0);
    let before = call.npv().unwrap();
    market.spot.set_value(110.0);
    let spot_price = call.npv().unwrap();
    assert!(spot_price > before);
    market.risk_free.set_value(0.06);
    let rate_price = call.npv().unwrap();
    assert!(rate_price > spot_price);
    market.dividend.set_value(0.04);
    let dividend_price = call.npv().unwrap();
    assert!(dividend_price < rate_price);
    let old_process = market.model.borrow().process();
    market.risk_handle.link_to(curve(&quote(0.09)));
    assert!(!Shared::ptr_eq(
        &old_process,
        &market.model.borrow().process()
    ));
    let relink_price = call.npv().unwrap();
    assert!(relink_price > dividend_price);
    let mut params = parameters();
    params[7] = 1.0;
    market.model.borrow_mut().set_params(&params).unwrap();
    assert!((call.npv().unwrap() - relink_price).abs() > 0.1);
}

#[test]
fn invalid_current_spot_raw_model_parameters_and_order_fail_loudly() {
    let market = Market::new(&parameters());
    for order in [0, 193, usize::MAX] {
        assert!(BatesEngine::new(market.model.clone(), order).is_err());
    }
    assert!(BatesEngine::with_default_order(market.model.clone()).is_ok());
    let mut call = market.option(OptionType::Call, 100.0);
    for spot in [0.0, -1.0, Real::NAN, Real::INFINITY] {
        market.spot.set_value(spot);
        assert!(call.npv().is_err());
        assert!(market.model.borrow().process().initial_values().is_err());
    }
    market.spot.set_value(100.0);
    let mut params = parameters();
    params[2] = Real::NAN;
    market
        .model
        .borrow_mut()
        .calibrated_model_mut()
        .set_params(&params)
        .unwrap();
    assert!(call.npv().is_err());
}

#[test]
fn unsupported_payoff_exercise_and_invalid_strike_fail_loudly() {
    let market = Market::new(&parameters());
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let cases: Vec<(Shared<dyn StrikedTypePayoff>, Shared<dyn Exercise>)> = vec![
        (
            shared(CashOrNothingPayoff::new(OptionType::Call, 100.0, 1.0)),
            shared(EuropeanExercise::new(reference() + 360)),
        ),
        (
            shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)),
            shared(AmericanExercise::new(reference(), reference() + 360, false).unwrap()),
        ),
        (
            shared(PlainVanillaPayoff::new(OptionType::Call, 0.0)),
            shared(EuropeanExercise::new(reference() + 360)),
        ),
    ];
    for (payoff, exercise) in cases {
        let mut option = VanillaOption::new(payoff, exercise, settings.clone());
        option.base_mut().set_pricing_engine(shared_mut(
            BatesEngine::new(market.model.clone(), 144).unwrap(),
        ) as SharedMut<dyn PricingEngine>);
        assert!(option.npv().is_err());
    }
}
