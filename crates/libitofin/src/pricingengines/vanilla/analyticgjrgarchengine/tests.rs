use super::*;
use crate::exercise::{AmericanExercise, EuropeanExercise};
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{CashOrNothingPayoff, EuropeanOption};
use crate::interestrate::Compounding;
use crate::math::array::Array;
use crate::pricingengines::vanilla::test_market::{Market, market, quote_handle, today};
use crate::processes::{GjrGarchDiscretization, GjrGarchParameters, GjrGarchProcess};
use crate::shared::{Shared, shared, shared_mut};
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;

struct Fixture {
    market: Market,
    model: SharedMut<GjrGarchModel>,
}

impl Fixture {
    fn new(lambda: Real) -> Self {
        let market = market();
        market.set(50.0, 0.0, 0.05, 0.2);
        let coefficients = Coefficients::new(0.93, 0.024, 0.059, lambda).unwrap();
        let parameters = GjrGarchParameters {
            omega: 2e-6,
            alpha: 0.024,
            beta: 0.93,
            gamma: 0.059,
            lambda,
            v0: 2e-6 / (1.0 - coefficients.m1),
            days_per_year: 365.0,
        };
        Self::from_parameters(market, parameters)
    }

    fn from_parameters(market: Market, parameters: GjrGarchParameters) -> Self {
        let curve = |quote| {
            Handle::new(shared(FlatForward::new(
                today(),
                quote_handle(quote),
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        let process = shared(
            GjrGarchProcess::new(
                curve(&market.r_rate),
                curve(&market.q_rate),
                market.process.state_variable(),
                parameters,
                GjrGarchDiscretization::FullTruncation,
            )
            .unwrap(),
        );
        let model = GjrGarchModel::new(process).unwrap();
        Self { market, model }
    }

    fn option(&self, option_type: OptionType, strike: Real, days: i32) -> EuropeanOption {
        let mut option = self.market.option(option_type, strike, today() + days);
        option
            .base_mut()
            .set_pricing_engine(shared_mut(AnalyticGjrGarchEngine::new(SharedMut::clone(
                &self.model,
            ))));
        option
    }

    fn price(&self, option_type: OptionType, strike: Real, days: i32) -> Real {
        self.option(option_type, strike, days).npv().unwrap()
    }
}

fn close(actual: Real, expected: Real, tolerance: Real) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual:.17}, expected={expected:.17}, tolerance={tolerance}"
    );
}

#[test]
fn reproduces_independent_quantlib_36_point_call_and_put_matrix() {
    for (lambda, rows) in [0.0, 0.1, 0.2]
        .into_iter()
        .zip(super::oracle_values::MATRIX)
    {
        let fixture = Fixture::new(lambda);
        for (days, strike, call, put) in rows {
            close(fixture.price(OptionType::Call, strike, days), call, 1e-10);
            close(fixture.price(OptionType::Put, strike, days), put, 1e-10);
        }
    }
}

#[test]
fn independent_quantlib_domain_variants_cover_fractional_days_and_skew() {
    for case in super::oracle_values::CASES {
        let [omega, alpha, beta, gamma, lambda, v0, days_per_year] = case.parameters;
        let market = market();
        market.set(100.0, case.rates[1], case.rates[0], 0.2);
        let fixture = Fixture::from_parameters(
            market,
            GjrGarchParameters {
                omega,
                alpha,
                beta,
                gamma,
                lambda,
                v0,
                days_per_year,
            },
        );
        let call = fixture.price(OptionType::Call, 100.0, case.days);
        let put = fixture.price(OptionType::Put, 100.0, case.days);
        assert!(
            (call - case.call).abs() <= 1e-10,
            "{} call: {call}, {}",
            case.name,
            case.call
        );
        assert!(
            (put - case.put).abs() <= 1e-10,
            "{} put: {put}, {}",
            case.name,
            case.put
        );
    }
}

#[test]
fn live_spot_rates_dividends_and_model_parameters_invalidate_instrument_and_moment_cache() {
    let fixture = Fixture::new(0.1);
    let mut option = fixture.option(OptionType::Call, 50.0, 90);
    let initial = option.npv().unwrap();
    assert!(option.delta().is_err());
    assert!(option.gamma().is_err());
    fixture.market.set(55.0, 0.0, 0.05, 0.2);
    let spot_changed = option.npv().unwrap();
    assert!((spot_changed - initial).abs() > 1.0);
    close(
        spot_changed,
        fixture.price(OptionType::Call, 50.0, 90),
        1e-12,
    );
    fixture.market.set(55.0, 0.0, 0.1, 0.2);
    let rate_changed = option.npv().unwrap();
    assert!((rate_changed - spot_changed).abs() > 0.01);
    close(
        rate_changed,
        fixture.price(OptionType::Call, 50.0, 90),
        1e-12,
    );
    fixture.market.set(55.0, 0.02, 0.1, 0.2);
    let dividend_changed = option.npv().unwrap();
    assert!((dividend_changed - rate_changed).abs() > 0.01);
    close(
        dividend_changed,
        fixture.price(OptionType::Call, 50.0, 90),
        1e-12,
    );
    let mut params = fixture.model.borrow().calibrated_model().params();
    params[0] *= 1.2;
    fixture.model.borrow_mut().set_params(&params).unwrap();
    let model_changed = option.npv().unwrap();
    assert!((model_changed - dividend_changed).abs() > 1e-4);
    close(
        model_changed,
        fixture.price(OptionType::Call, 50.0, 90),
        1e-12,
    );
}

#[test]
fn nonzero_dividend_source_put_parity_is_preserved() {
    let fixture = Fixture::new(0.0);
    fixture.market.set(50.0, 0.03, 0.05, 0.2);
    let call = fixture.price(OptionType::Call, 55.0, 90);
    let put = fixture.price(OptionType::Put, 55.0, 90);
    close(
        put - call,
        55.0 * (-0.02_f64 * 90.0 / 365.0).exp() - 50.0,
        1e-12,
    );
}

#[test]
fn horizon_and_expansion_domain_failures_are_explicit() {
    let fixture = Fixture::new(0.0);
    for strike in [0.0, -1.0, Real::NAN, Real::INFINITY] {
        assert!(fixture.option(OptionType::Call, strike, 90).npv().is_err());
    }
    assert!(fixture.option(OptionType::Call, 50.0, 1001).npv().is_err());
    let mut engine = AnalyticGjrGarchEngine::new(SharedMut::clone(&fixture.model));
    engine.base.arguments_mut().payoff =
        Some(shared(PlainVanillaPayoff::new(OptionType::Call, 50.0)));
    engine.base.arguments_mut().exercise = Some(shared(EuropeanExercise::new(today())));
    assert!(engine.calculate().is_err());
    assert!(Coefficients::new(0.0, 0.0, 0.0, 0.0).is_err());
    assert!(Coefficients::new(1.0, 0.0, 0.0, 0.0).is_err());
    assert!(Coefficients::new(1.0 - Real::EPSILON, 0.0, 0.0, 0.0).is_err());
    assert!(Coefficients::new(0.93, 0.024, 0.059, Real::MAX).is_err());
    let c = Coefficients::new(0.93, 0.024, 0.059, 0.0).unwrap();
    assert!(Moments::new(c, 0.0, 0.0, 90, 0.0).is_err());
    assert!(Moments::new(c, Real::MAX, Real::MAX, 1, 0.0).is_err());
    let m = Moments {
        mean: 0.0,
        variance: 0.04,
        skewness: Real::MAX,
        kurtosis: 3.0,
    };
    assert!(approximate_call(m, Real::MAX, 50.0, 0.0).is_err());
    let negative = Moments {
        mean: 0.0,
        variance: 0.04,
        skewness: -100.0,
        kurtosis: 3.0,
    };
    assert!(approximate_call(negative, 50.0, 50.0, 0.0).unwrap() < 0.0);
}

#[test]
fn unsupported_payoff_and_exercise_are_rejected() {
    let fixture = Fixture::new(0.0);
    let mut engine = AnalyticGjrGarchEngine::new(SharedMut::clone(&fixture.model));
    assert!(engine.calculate().is_err());
    engine.base.arguments_mut().exercise = Some(shared(EuropeanExercise::new(today() + 90)));
    assert!(engine.calculate().is_err());
    engine.base.arguments_mut().payoff = Some(shared(CashOrNothingPayoff::new(
        OptionType::Call,
        50.0,
        1.0,
    )));
    assert!(engine.calculate().is_err());
    engine.base.arguments_mut().payoff =
        Some(shared(PlainVanillaPayoff::new(OptionType::Call, 50.0)));
    engine.base.arguments_mut().exercise = Some(shared(
        AmericanExercise::new(today(), today() + 90, false).unwrap(),
    ));
    assert!(engine.calculate().is_err());
    engine.base.arguments_mut().exercise = Some(shared(EuropeanExercise::new(today() + 90)));
    engine.calculate().unwrap();
    assert!(engine.base.results().instrument.value.unwrap().is_finite());
    engine.reset();
    assert!(engine.base.results().instrument.value.is_none());
    fixture.market.set(0.0, 0.0, 0.05, 0.2);
    assert!(engine.calculate().is_err());
    fixture.market.set(50.0, 0.0, 0.05, 0.2);
    engine.calculate().unwrap();
}

#[test]
fn singular_model_parameters_fail_without_replacing_the_engine() {
    let fixture = Fixture::new(0.0);
    let mut option = fixture.option(OptionType::Call, 50.0, 90);
    let value = option.npv().unwrap();
    fixture
        .model
        .borrow_mut()
        .set_params(&Array::from([2e-6, 0.0, 0.0, 0.0, 0.0, 0.0002]))
        .unwrap();
    assert!(option.npv().is_err());
    let reset = Fixture::new(0.0).model.borrow().calibrated_model().params();
    fixture.model.borrow_mut().set_params(&reset).unwrap();
    close(option.npv().unwrap(), value, 1e-12);
}

#[test]
fn reused_engine_does_not_cache_strike_exercise_or_market_errors() {
    let fixture = Fixture::new(0.1);
    let mut engine = AnalyticGjrGarchEngine::new(SharedMut::clone(&fixture.model));
    for (days, strike) in [(90, 50.0), (90, 55.0), (180, 55.0), (90, 50.0)] {
        engine.base.arguments_mut().payoff =
            Some(shared(PlainVanillaPayoff::new(OptionType::Call, strike)));
        engine.base.arguments_mut().exercise = Some(shared(EuropeanExercise::new(today() + days)));
        engine.calculate().unwrap();
        close(
            engine.base.results().instrument.value.unwrap(),
            fixture.price(OptionType::Call, strike, days),
            1e-12,
        );
    }
    for (spot, rate) in [
        (Real::INFINITY, 0.05),
        (Real::NAN, 0.05),
        (50.0, 1e8),
        (50.0, -1e8),
    ] {
        fixture.market.set(spot, 0.0, rate, 0.2);
        assert!(engine.calculate().is_err());
        fixture.market.set(50.0, 0.0, 0.05, 0.2);
        engine.calculate().unwrap();
    }
    let result = engine.base.results();
    assert!(result.greeks.delta.is_none());
    assert!(result.greeks.gamma.is_none());
    assert!(result.greeks.theta.is_none());
    assert!(result.greeks.vega.is_none());
    assert!(result.greeks.rho.is_none());
    assert!(result.greeks.dividend_rho.is_none());
}
