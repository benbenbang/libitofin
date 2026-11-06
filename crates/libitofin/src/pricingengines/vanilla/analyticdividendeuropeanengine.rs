//! Analytic engine for European options with discrete dividends.
//!
//! Port of `ql/pricingengines/vanilla/analyticdividendeuropeanengine.{hpp,cpp}`:
//! the spot is reduced by the present value of the dividends paid between
//! the settlement date and the exercise date, both ends included, and the
//! Black formula is applied to the resulting forward.
//!
//! Deviations, documented per D10:
//! - The C++ "non-striked payoff" check is enforced by the type of the
//!   option arguments, which only hold striked payoffs.
//! - As in C++, no dividend rho is set; a theta the calculator cannot
//!   produce is left unset, so asking the option for it is an `Err`.

use crate::cashflows::Dividend;
use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::fail;
use crate::instruments::{Greeks, OneAssetOptionEngine, OneAssetOptionResults, OptionArguments};
use crate::interestrate::Compounding;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::BlackCalculator;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::Shared;
use crate::time::frequency::Frequency;

/// Pricing engine for European vanilla options on a stock paying discrete
/// dividends, using analytical formulae.
pub struct AnalyticDividendEuropeanEngine {
    base: OneAssetOptionEngine,
    process: Shared<GeneralizedBlackScholesProcess>,
    dividends: Vec<Shared<dyn Dividend>>,
}

impl AnalyticDividendEuropeanEngine {
    /// Builds the engine on a Black-Scholes process and a dividend schedule.
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        dividends: Vec<Shared<dyn Dividend>>,
    ) -> AnalyticDividendEuropeanEngine {
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(process.observable());
        AnalyticDividendEuropeanEngine {
            base,
            process,
            dividends,
        }
    }
}

impl AsObservable for AnalyticDividendEuropeanEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for AnalyticDividendEuropeanEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        if exercise.exercise_type() != ExerciseType::European {
            fail!("not an European option");
        }
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let payoff = Shared::clone(payoff);
        let maturity_date = exercise.last_date();

        let risk_free = self.process.risk_free_rate().current_link()?;
        let dividend = self.process.dividend_yield().current_link()?;
        let black_vol = self.process.black_volatility().current_link()?;

        let settlement_date = risk_free.reference_date()?;
        let paid: Vec<&Shared<dyn Dividend>> = self
            .dividends
            .iter()
            .filter(|d| d.date() >= settlement_date && d.date() <= maturity_date)
            .collect();

        let mut riskless = 0.0;
        for d in &paid {
            let date = d.date();
            riskless += d.amount()? * risk_free.discount_date(date, false)?
                / dividend.discount_date(date, false)?;
        }

        let spot = self.process.state_variable().current_link()?.value()? - riskless;
        if spot.is_nan() || spot <= 0.0 {
            fail!("negative or null underlying after subtracting dividends");
        }

        let dividend_discount = dividend.discount_date(maturity_date, false)?;
        let risk_free_discount = risk_free.discount_date(maturity_date, false)?;
        let forward_price = spot * dividend_discount / risk_free_discount;
        let variance = black_vol.black_variance_date(maturity_date, payoff.strike(), false)?;

        let black = BlackCalculator::with_striked_payoff(
            &*payoff,
            forward_price,
            variance.sqrt(),
            risk_free_discount,
        )?;

        let value = black.value();
        let delta = black.delta(spot)?;
        let gamma = black.gamma(spot)?;

        let rfdc = risk_free.require_day_counter()?;
        let dydc = dividend.require_day_counter()?;
        let vega = black.vega(black_vol.time_from_reference(maturity_date)?)?;

        let mut delta_theta = 0.0;
        let mut delta_rho = 0.0;
        for d in &paid {
            let date = d.date();
            let amount = d.amount()?;
            let r = risk_free.zero_rate_date(
                date,
                rfdc.clone(),
                Compounding::Continuous,
                Frequency::Annual,
                false,
            )?;
            let q = dividend.zero_rate_date(
                date,
                dydc.clone(),
                Compounding::Continuous,
                Frequency::Annual,
                false,
            )?;
            delta_theta -= amount * (r.rate() - q.rate()) * risk_free.discount_date(date, false)?
                / dividend.discount_date(date, false)?;

            let t = risk_free.time_from_reference(date)?;
            delta_rho +=
                amount * t * risk_free.discount(t, false)? / dividend.discount(t, false)?;
        }

        let t = risk_free.time_from_reference(maturity_date)?;
        let theta = black
            .theta(spot, t)
            .ok()
            .map(|theta| theta + delta_theta * delta);
        let rho = black.rho(t)? + delta_rho * delta;

        let results = self.base.results_mut();
        results.instrument.value = Some(value);
        results.greeks = Greeks {
            delta: Some(delta),
            gamma: Some(gamma),
            theta,
            vega: Some(vega),
            rho: Some(rho),
            dividend_rho: None,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! The analytic oracles of `test-suite/dividendoption.cpp`, on curves
    //! moving off an explicit evaluation date as `flatRate`/`flatVol` do.

    use super::AnalyticDividendEuropeanEngine;
    use crate::cashflows::dividend_vector;
    use crate::exercise::{AmericanExercise, EuropeanExercise, Exercise};
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
    use crate::interestrate::Compounding;
    use crate::option::OptionType;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::vanilla::AnalyticEuropeanEngine;
    use crate::pricingengines::vanilla::test_market::{quote_handle, today};
    use crate::processes::{BlackScholesMertonProcess, GeneralizedBlackScholesProcess};
    use crate::quotes::SimpleQuote;
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::NullCalendar;
    use crate::time::date::Date;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Rate, Real, Volatility};

    const U: Real = 100.0;
    const TOLERANCE: Real = 1.0e-5;

    struct Market {
        settings: Shared<Settings<Date>>,
        spot: Shared<SimpleQuote>,
        q_rate: Shared<SimpleQuote>,
        r_rate: Shared<SimpleQuote>,
        vol: Shared<SimpleQuote>,
        process: Shared<GeneralizedBlackScholesProcess>,
    }

    impl Market {
        fn new() -> Market {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(today());
            let quote = || shared(SimpleQuote::new(0.0));
            let (spot, q_rate, r_rate, vol) = (quote(), quote(), quote(), quote());
            let flat = |q: &Shared<SimpleQuote>| {
                Handle::new(shared(FlatForward::moving(
                    0,
                    NullCalendar::new(),
                    quote_handle(q),
                    Actual360::new(),
                    Compounding::Continuous,
                    Frequency::Annual,
                    Shared::clone(&settings),
                )) as Shared<dyn YieldTermStructure>)
            };
            let vol_ts = shared(BlackConstantVol::moving_with_quote(
                0,
                NullCalendar::new(),
                quote_handle(&vol),
                Actual360::new(),
                Shared::clone(&settings),
            )) as Shared<dyn BlackVolTermStructure>;
            let process = shared(BlackScholesMertonProcess::new(
                quote_handle(&spot),
                flat(&q_rate),
                flat(&r_rate),
                Handle::new(vol_ts),
            ));
            Market {
                settings,
                spot,
                q_rate,
                r_rate,
                vol,
                process,
            }
        }

        fn set(&self, q: Real, r: Real, v: Real) {
            self.spot.set_value(U);
            self.q_rate.set_value(q);
            self.r_rate.set_value(r);
            self.vol.set_value(v);
        }

        fn option(
            &self,
            engine: SharedMut<dyn PricingEngine>,
            option_type: OptionType,
            strike: Real,
            exercise: Shared<dyn Exercise>,
        ) -> EuropeanOption {
            let payoff = shared(PlainVanillaPayoff::new(option_type, strike));
            let mut option = EuropeanOption::new(payoff, exercise, Shared::clone(&self.settings));
            option.base_mut().set_pricing_engine(engine);
            option
        }

        fn dividend(
            &self,
            option_type: OptionType,
            strike: Real,
            expiry: Date,
            dates: &[Date],
            amounts: &[Real],
        ) -> EuropeanOption {
            let dividends = dividend_vector(dates, amounts).unwrap();
            let engine = shared_mut(AnalyticDividendEuropeanEngine::new(
                Shared::clone(&self.process),
                dividends,
            ));
            self.option(
                engine,
                option_type,
                strike,
                shared(EuropeanExercise::new(expiry)),
            )
        }

        fn reference(&self, option_type: OptionType, strike: Real, expiry: Date) -> EuropeanOption {
            let engine = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&self.process)));
            self.option(
                engine,
                option_type,
                strike,
                shared(EuropeanExercise::new(expiry)),
            )
        }
    }

    fn contracts() -> Vec<(OptionType, Real, Date)> {
        let mut contracts = Vec::new();
        for option_type in [OptionType::Call, OptionType::Put] {
            for strike in [50.0, 99.5, 100.0, 100.5, 150.0] {
                for length in [1, 2] {
                    let expiry = today() + Period::new(length, TimeUnit::Years);
                    contracts.push((option_type, strike, expiry));
                }
            }
        }
        contracts
    }

    fn scenarios(vols: [Volatility; 3]) -> Vec<(Rate, Rate, Volatility)> {
        let mut scenarios = Vec::new();
        for q in [0.00, 0.10, 0.30] {
            for r in [0.01, 0.05, 0.15] {
                for v in vols {
                    scenarios.push((q, r, v));
                }
            }
        }
        scenarios
    }

    fn semiannual_dates(expiry: Date) -> Vec<Date> {
        let mut dates = Vec::new();
        let mut d = today() + Period::new(3, TimeUnit::Months);
        while d < expiry {
            dates.push(d);
            d = d + Period::new(6, TimeUnit::Months);
        }
        dates
    }

    #[test]
    fn no_dividends_in_the_window_match_the_plain_european_engine() {
        let m = Market::new();
        for (option_type, strike, expiry) in contracts() {
            let dates = semiannual_dates(expiry);
            let mut reference = m.reference(option_type, strike, expiry);
            let mut zero_amounts =
                m.dividend(option_type, strike, expiry, &dates, &vec![0.0; dates.len()]);
            let after_expiry = expiry + Period::new(6, TimeUnit::Months);
            let mut outside = m.dividend(option_type, strike, expiry, &[after_expiry], &[1.0]);
            for (q, r, v) in scenarios([0.05, 0.20, 0.70]) {
                m.set(q, r, v);
                let expected = reference.npv().unwrap();
                for calculated in [zero_amounts.npv().unwrap(), outside.npv().unwrap()] {
                    assert!(
                        (calculated - expected).abs() <= TOLERANCE,
                        "{option_type:?} K={strike} {expiry:?} q={q} r={r} v={v}: \
                         {calculated} vs {expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn dividend_on_the_settlement_date_comes_straight_off_the_spot() {
        let dividend = 10.0;
        let m = Market::new();
        let settlement = m
            .process
            .risk_free_rate()
            .current_link()
            .unwrap()
            .reference_date()
            .unwrap();
        assert_eq!(settlement, today());
        for (option_type, strike, expiry) in contracts() {
            let mut reference = m.reference(option_type, strike, expiry);
            let mut option = m.dividend(option_type, strike, expiry, &[today()], &[dividend]);
            for (q, r, v) in scenarios([0.05, 0.20, 0.70]) {
                m.set(q, r, v);
                let calculated = option.npv().unwrap();
                m.spot.set_value(U - dividend);
                let expected = reference.npv().unwrap();
                assert!(
                    (calculated - expected).abs() <= TOLERANCE,
                    "{option_type:?} K={strike} {expiry:?} q={q} r={r} v={v}: \
                     {calculated} vs {expected}"
                );
            }
        }
    }

    /// Not a QuantLib oracle: `testEuropeanEndLimit` is commented out in the
    /// test suite (`dividendoption.cpp:303`, "need to use discounted values").
    /// This pins the closed window of `analyticdividendeuropeanengine.cpp:49-50`
    /// using those discounted values.
    #[test]
    fn dividend_window_is_closed_on_both_ends() {
        let dividend = 10.0;
        let (q, r, v) = (0.10, 0.05, 0.20);
        let m = Market::new();
        for (option_type, strike, expiry) in contracts() {
            let mut reference = m.reference(option_type, strike, expiry);
            let mut on_expiry = m.dividend(option_type, strike, expiry, &[expiry], &[dividend]);
            let mut outside = m.dividend(
                option_type,
                strike,
                expiry,
                &[today() - 1, expiry + 1],
                &[dividend, dividend],
            );
            m.set(q, r, v);
            let undividended = reference.npv().unwrap();
            let calculated = outside.npv().unwrap();
            assert!(
                (calculated - undividended).abs() <= TOLERANCE,
                "{option_type:?} K={strike} {expiry:?}: {calculated} vs {undividended}"
            );

            let risk_free = m.process.risk_free_rate().current_link().unwrap();
            let yield_curve = m.process.dividend_yield().current_link().unwrap();
            let pv = dividend * risk_free.discount_date(expiry, false).unwrap()
                / yield_curve.discount_date(expiry, false).unwrap();
            let calculated = on_expiry.npv().unwrap();
            m.spot.set_value(U - pv);
            let expected = reference.npv().unwrap();
            assert!(
                (calculated - expected).abs() <= TOLERANCE,
                "{option_type:?} K={strike} {expiry:?}: {calculated} vs {expected}"
            );
        }
    }

    #[test]
    fn greeks_are_consistent_with_finite_differences() {
        let m = Market::new();
        for (option_type, strike, expiry) in contracts() {
            let dates = semiannual_dates(expiry);
            let amounts = vec![5.0; dates.len()];
            let mut option = m.dividend(option_type, strike, expiry, &dates, &amounts);
            for (q, r, v) in scenarios([0.05, 0.20, 0.40]) {
                m.set(q, r, v);
                let value = option.npv().unwrap();
                let delta = option.delta().unwrap();
                let gamma = option.gamma().unwrap();
                let theta = option.theta().unwrap();
                let rho = option.rho().unwrap();
                let vega = option.vega().unwrap();

                if value <= U * 1.0e-5 {
                    continue;
                }

                let du = U * 1.0e-4;
                m.spot.set_value(U + du);
                let value_p = option.npv().unwrap();
                let delta_p = option.delta().unwrap();
                m.spot.set_value(U - du);
                let value_m = option.npv().unwrap();
                let delta_m = option.delta().unwrap();
                m.spot.set_value(U);
                let expected_delta = (value_p - value_m) / (2.0 * du);
                let expected_gamma = (delta_p - delta_m) / (2.0 * du);

                let dr = r * 1.0e-4;
                m.r_rate.set_value(r + dr);
                let value_p = option.npv().unwrap();
                m.r_rate.set_value(r - dr);
                let value_m = option.npv().unwrap();
                m.r_rate.set_value(r);
                let expected_rho = (value_p - value_m) / (2.0 * dr);

                let dv = v * 1.0e-4;
                m.vol.set_value(v + dv);
                let value_p = option.npv().unwrap();
                m.vol.set_value(v - dv);
                let value_m = option.npv().unwrap();
                m.vol.set_value(v);
                let expected_vega = (value_p - value_m) / (2.0 * dv);

                let dt = Actual360::new().year_fraction(today() - 1, today() + 1);
                m.settings.set_evaluation_date(today() - 1);
                let value_m = option.npv().unwrap();
                m.settings.set_evaluation_date(today() + 1);
                let value_p = option.npv().unwrap();
                m.settings.set_evaluation_date(today());
                let expected_theta = (value_p - value_m) / dt;

                let checks = [
                    ("delta", expected_delta, delta),
                    ("gamma", expected_gamma, gamma),
                    ("theta", expected_theta, theta),
                    ("rho", expected_rho, rho),
                    ("vega", expected_vega, vega),
                ];
                for (name, expected, calculated) in checks {
                    let error = (expected - calculated).abs() / U;
                    assert!(
                        error <= TOLERANCE,
                        "{name} of {option_type:?} K={strike} {expiry:?} q={q} r={r} v={v}: \
                         analytic {calculated} vs finite difference {expected} \
                         (relative error {error})"
                    );
                }
            }
        }
    }

    #[test]
    fn hull_exercise_12_8_known_value() {
        let m = Market::new();
        let dates = [today() + 60, today() + 150];
        let mut option = m.dividend(OptionType::Call, 40.0, today() + 180, &dates, &[0.50, 0.50]);
        m.spot.set_value(40.0);
        m.r_rate.set_value(0.09);
        m.vol.set_value(0.30);
        let calculated = option.npv().unwrap();
        assert!((calculated - 3.67).abs() <= 1.0e-2, "{calculated}");
        assert!(option.dividend_rho().is_err());
    }

    #[test]
    fn non_european_exercise_is_an_error() {
        let m = Market::new();
        let engine = shared_mut(AnalyticDividendEuropeanEngine::new(
            Shared::clone(&m.process),
            dividend_vector(&[today() + 30], &[1.0]).unwrap(),
        ));
        let exercise = shared(AmericanExercise::new(today(), today() + 360, false).unwrap());
        let mut option = m.option(engine, OptionType::Call, 100.0, exercise);
        m.set(0.0, 0.05, 0.20);
        assert!(option.npv().is_err());
    }

    #[test]
    fn dividends_exceeding_the_spot_are_an_error() {
        let m = Market::new();
        let mut option = m.dividend(
            OptionType::Call,
            100.0,
            today() + 360,
            &[today() + 30],
            &[150.0],
        );
        m.set(0.0, 0.05, 0.20);
        let error = option.npv().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("negative or null underlying after subtracting dividends"),
            "{error}"
        );
    }
}
