use super::*;
use crate::exercise::{AmericanExercise, EuropeanExercise};
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{CashOrNothingPayoff, EuropeanOption, StrikedTypePayoff};
use crate::option::OptionType;
use crate::pricingengines::vanilla::test_market::{
    Market, market, quote_handle, time_to_days, today,
};
use crate::quotes::SimpleQuote;
use crate::shared::{shared, shared_mut};

struct JumpMarket {
    market: Market,
    intensity: Shared<SimpleQuote>,
    mean: Shared<SimpleQuote>,
    jump_vol: Shared<SimpleQuote>,
    process: Shared<Merton76Process>,
}

impl JumpMarket {
    fn new(intensity: Real, mean: Real, jump_vol: Real) -> Self {
        let market = market();
        market.set(100.0, 0.02, 0.05, 0.20);
        let intensity = shared(SimpleQuote::new(intensity));
        let mean = shared(SimpleQuote::new(mean));
        let jump_vol = shared(SimpleQuote::new(jump_vol));
        let process = shared(
            Merton76Process::from_black_scholes(
                Shared::clone(&market.process),
                quote_handle(&intensity),
                quote_handle(&mean),
                quote_handle(&jump_vol),
            )
            .unwrap(),
        );
        Self {
            market,
            intensity,
            mean,
            jump_vol,
            process,
        }
    }

    fn option(
        &self,
        option_type: OptionType,
        strike: Real,
        tolerance: Real,
        budget: usize,
    ) -> EuropeanOption {
        let mut option = self
            .market
            .option(option_type, strike, today() + time_to_days(1.0));
        let engine =
            JumpDiffusionEngine::new(Shared::clone(&self.process), tolerance, budget).unwrap();
        option
            .base_mut()
            .set_pricing_engine(shared_mut(engine) as crate::shared::SharedMut<dyn PricingEngine>);
        option
    }
}

fn close(actual: Real, expected: Real, tolerance: Real) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual:.17}, expected={expected:.17}, tolerance={tolerance}"
    );
}

fn values(option: &mut EuropeanOption) -> [Real; 7] {
    [
        option.npv().unwrap(),
        option.delta().unwrap(),
        option.gamma().unwrap(),
        option.theta().unwrap(),
        option.vega().unwrap(),
        option.rho().unwrap(),
        option.dividend_rho().unwrap(),
    ]
}

#[test]
fn positive_jump_intensity_underflow_is_rejected_and_recoverable() {
    let m = JumpMarket::new(1.0, -744.0, 0.0);
    m.market.set(100.0, 0.0, 0.0, 0.0);
    let mut option = m.market.option(OptionType::Put, 100.0, today() + 1);
    let engine = JumpDiffusionEngine::new(Shared::clone(&m.process), 1e-12, 1000).unwrap();
    option.base_mut().set_pricing_engine(shared_mut(engine));
    assert!(option.npv().is_err());
    m.mean.set_value(0.0);
    close(option.npv().unwrap(), 0.0, 0.0);
    m.intensity.set_value(1e-300);
    m.mean.set_value(-100.0);
    assert!(
        Merton76Process::from_black_scholes(
            Shared::clone(&m.market.process),
            quote_handle(&m.intensity),
            quote_handle(&m.mean),
            quote_handle(&m.jump_vol),
        )
        .is_err()
    );
}

#[test]
fn zero_jump_is_black_scholes_including_greeks() {
    for option_type in [OptionType::Call, OptionType::Put] {
        let m = JumpMarket::new(0.0, -0.10, 0.30);
        let mut jump = m.option(option_type, 100.0, 1e-12, 1);
        let mut black = m
            .market
            .option(option_type, 100.0, today() + time_to_days(1.0));
        for (actual, expected) in values(&mut jump).into_iter().zip(values(&mut black)) {
            close(actual, expected, 1e-12);
        }
        close(
            jump.theta_per_day().unwrap(),
            jump.theta().unwrap() / 365.0,
            0.0,
        );
    }
}

#[test]
fn haug_merton_cached_prices_preserve_quantlib_tolerance() {
    for (strike, intensity, t, expected) in [
        (80.0, 1.0, 0.10, 20.67),
        (100.0, 5.0, 0.25, 5.96),
        (110.0, 10.0, 0.50, 4.74),
        (120.0, 1.0, 0.50, 2.23),
    ] {
        let gamma: Real = 0.25;
        let diffusion = 0.25 * (1.0 - gamma).sqrt();
        let jump_vol = 0.25 * (gamma / intensity).sqrt();
        let m = JumpMarket::new(intensity, -0.5 * jump_vol * jump_vol, jump_vol);
        m.market.set(100.0, 0.0, 0.08, diffusion);
        let mut option = m
            .market
            .option(OptionType::Call, strike, today() + time_to_days(t));
        option
            .base_mut()
            .set_pricing_engine(shared_mut(
                JumpDiffusionEngine::new(Shared::clone(&m.process), 1e-12, 1000).unwrap(),
            ) as crate::shared::SharedMut<dyn PricingEngine>);
        close(option.npv().unwrap(), expected, 1e-2);
    }
}

#[test]
fn analytic_greeks_agree_with_price_derivatives() {
    let m = JumpMarket::new(0.7, -0.12, 0.3);
    let mut option = m.option(OptionType::Call, 100.0, 1e-14, 1000);
    let expected = values(&mut option);
    let h = 1e-3;
    m.market.spot.set_value(100.0 + h);
    let plus = option.npv().unwrap();
    m.market.spot.set_value(100.0 - h);
    let minus = option.npv().unwrap();
    m.market.spot.set_value(100.0);
    close((plus - minus) / (2.0 * h), expected[1], 2e-8);
    close(
        (plus + minus - 2.0 * expected[0]) / (h * h),
        expected[2],
        2e-7,
    );
    for (quote, value, greek) in [
        (&m.market.vol, 0.20, 4),
        (&m.market.r_rate, 0.05, 5),
        (&m.market.q_rate, 0.02, 6),
    ] {
        let h = 1e-5;
        quote.set_value(value + h);
        let plus = option.npv().unwrap();
        quote.set_value(value - h);
        let minus = option.npv().unwrap();
        quote.set_value(value);
        close((plus - minus) / (2.0 * h), expected[greek], 1e-7);
    }
}

#[test]
fn live_jump_and_market_quotes_invalidate_cached_option() {
    let m = JumpMarket::new(0.7, -0.12, 0.3);
    let mut option = m.option(OptionType::Call, 100.0, 1e-12, 1000);
    let original = option.npv().unwrap();
    for (quote, changed, initial) in [
        (&m.intensity, 1.2, 0.7),
        (&m.mean, -0.25, -0.12),
        (&m.jump_vol, 0.45, 0.3),
        (&m.market.spot, 110.0, 100.0),
        (&m.market.r_rate, 0.08, 0.05),
        (&m.market.q_rate, 0.04, 0.02),
        (&m.market.vol, 0.30, 0.20),
    ] {
        quote.set_value(changed);
        assert!((option.npv().unwrap() - original).abs() > 1e-5);
        quote.set_value(initial);
        close(option.npv().unwrap(), original, 0.0);
    }
}

#[test]
fn invalid_live_quotes_fail_and_recover() {
    let m = JumpMarket::new(0.7, -0.12, 0.3);
    let mut option = m.option(OptionType::Call, 100.0, 1e-12, 1000);
    let original = option.npv().unwrap();
    for (quote, invalid, initial) in [
        (&m.intensity, -1.0, 0.7),
        (&m.intensity, Real::INFINITY, 0.7),
        (&m.mean, Real::NAN, -0.12),
        (&m.mean, 1000.0, -0.12),
        (&m.jump_vol, -0.1, 0.3),
        (&m.jump_vol, Real::MAX, 0.3),
        (&m.market.spot, Real::INFINITY, 100.0),
        (&m.market.spot, 0.0, 100.0),
        (&m.market.vol, Real::INFINITY, 0.20),
        (&m.market.vol, -0.1, 0.20),
    ] {
        quote.set_value(invalid);
        assert!(option.npv().is_err());
        quote.set_value(initial);
        close(option.npv().unwrap(), original, 0.0);
    }
}

#[test]
fn parameters_and_series_budget_are_strictly_bounded() {
    let m = JumpMarket::new(0.7, -0.12, 0.3);
    for tolerance in [0.0, -1.0, Real::NAN, Real::INFINITY] {
        assert!(JumpDiffusionEngine::new(Shared::clone(&m.process), tolerance, 1000).is_err());
    }
    for budget in [0, 100001, usize::MAX] {
        assert!(JumpDiffusionEngine::new(Shared::clone(&m.process), 1e-12, budget).is_err());
    }
    assert!(m.option(OptionType::Call, 100.0, 1e-12, 1).npv().is_err());
    assert!(m.option(OptionType::Call, 100.0, 2.0, 1000).npv().unwrap() > 0.0);
    m.intensity.set_value(100.0);
    assert!(m.option(OptionType::Call, 100.0, 1e-12, 10).npv().is_err());
    m.mean.set_value(-10.0);
    m.jump_vol.set_value(0.0);
    assert!(m.option(OptionType::Put, 100.0, 1e-12, 10).npv().is_err());
    assert!(
        Merton76Process::from_black_scholes(
            Shared::clone(&m.market.process),
            Handle::empty(),
            quote_handle(&m.mean),
            quote_handle(&m.jump_vol)
        )
        .is_err()
    );
}

#[test]
fn zero_diffusion_and_zero_strike_do_not_create_nan_greeks() {
    for (intensity, mean, jump_vol) in [(0.0, 0.0, 0.0), (0.7, -0.12, 0.3), (0.7, -0.12, 0.0)] {
        let m = JumpMarket::new(intensity, mean, jump_vol);
        m.market.vol.set_value(0.0);
        for option_type in [OptionType::Call, OptionType::Put] {
            for strike in [0.0, 100.0, 120.0] {
                assert!(
                    values(&mut m.option(option_type, strike, 1e-12, 1000))
                        .iter()
                        .all(|x| x.is_finite())
                );
            }
        }
    }
}

#[test]
fn unsupported_payoffs_exercises_and_path_methods_return_errors() {
    let m = JumpMarket::new(0.7, -0.12, 0.3);
    assert!(m.process.drift(0.0, 100.0).is_err());
    assert!(m.process.diffusion(0.0, 100.0).is_err());
    assert!(m.process.apply(100.0, 0.1).is_err());
    close(m.process.x0().unwrap(), 100.0, 0.0);
    close(m.process.time(&(today() + 360)).unwrap(), 1.0, 0.0);
    let mut engine = JumpDiffusionEngine::new(Shared::clone(&m.process), 1e-12, 1000).unwrap();
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.payoff = Some(
        shared(CashOrNothingPayoff::new(OptionType::Call, 100.0, 10.0))
            as Shared<dyn StrikedTypePayoff>,
    );
    args.exercise = Some(shared(EuropeanExercise::new(today() + 360)));
    assert!(engine.calculate().is_err());
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    args.payoff = Some(shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)));
    args.exercise = Some(shared(
        AmericanExercise::new(today(), today() + 360, false).unwrap(),
    ));
    assert!(engine.calculate().is_err());
}
