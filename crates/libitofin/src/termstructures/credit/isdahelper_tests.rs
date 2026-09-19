//! Independent QuantLib 1.43 helper bootstrap pins and compatibility checks.
//! `tests/fixtures/isda_helpers/generator.py` records the market and provenance.

use super::*;
use crate::handle::Handle;
use crate::instruments::PricingModel;
use crate::interestrate::Compounding;
use crate::math::interpolations::{flat::BackwardFlat, loglinear::LogLinear};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::shared;
use crate::termstructures::credit::defaultprobabilityhelpers::{
    CdsHelperTerms, SpreadCdsHelper, UpfrontCdsHelper,
};
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::target::Target;
use crate::time::date::Month;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

const ORACLE: &str = include_str!("../../../tests/fixtures/isda_helpers/oracle.csv");

struct Market {
    settings: Shared<Settings<Date>>,
    rate: Shared<SimpleQuote>,
    quotes: Vec<Shared<SimpleQuote>>,
    helpers: Vec<Shared<dyn DefaultProbabilityHelper>>,
}

fn today() -> Date {
    Date::new(15, Month::June, 2026)
}

fn market(upfront: bool, valid_discount: bool, settles_accrual: bool, lag: u32) -> Market {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today());
    let rate = shared(SimpleQuote::new(0.03));
    let discount = Handle::new(shared(FlatForward::new(
        today(),
        Handle::new(rate.clone() as Shared<dyn Quote>),
        if valid_discount {
            Actual365Fixed::new()
        } else {
            Actual360::new()
        },
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    let values = if upfront {
        [0.01, 0.02, 0.04]
    } else {
        [0.005, 0.01, 0.015]
    };
    let quotes: Vec<_> = values
        .into_iter()
        .map(|value| shared(SimpleQuote::new(value)))
        .collect();
    let helpers = [1, 3, 5]
        .into_iter()
        .zip(&quotes)
        .map(|(years, quote)| {
            let terms = CdsHelperTerms {
                model: PricingModel::Isda,
                last_period_day_counter: Some(Actual360::with_last_day(true)),
                settles_accrual,
                ..CdsHelperTerms::default()
            };
            let quote = Handle::new(quote.clone() as Shared<dyn Quote>);
            if upfront {
                UpfrontCdsHelper::with_terms(
                    quote,
                    0.01,
                    Period::new(years, TimeUnit::Years),
                    if lag == 0 { 0 } else { 1 },
                    Target::new(),
                    Frequency::Quarterly,
                    BusinessDayConvention::Following,
                    DateGeneration::CDS,
                    Actual360::new(),
                    0.4,
                    discount.clone(),
                    lag,
                    terms,
                    settings.clone(),
                )
                .unwrap() as Shared<dyn DefaultProbabilityHelper>
            } else {
                SpreadCdsHelper::with_terms(
                    quote,
                    Period::new(years, TimeUnit::Years),
                    if lag == 0 { 0 } else { 1 },
                    Target::new(),
                    Frequency::Quarterly,
                    BusinessDayConvention::Following,
                    DateGeneration::CDS,
                    Actual360::new(),
                    0.4,
                    discount.clone(),
                    terms,
                    settings.clone(),
                )
                .unwrap() as Shared<dyn DefaultProbabilityHelper>
            }
        })
        .collect();
    Market {
        settings,
        rate,
        quotes,
        helpers,
    }
}

fn check_oracle<T: CreditBootstrapTraits + 'static, I: Interpolator + 'static>(
    upfront: bool,
    lag: u32,
    interpolator: I,
) {
    let market = market(upfront, true, true, lag);
    let curve = PiecewiseDefaultCurve::<T, I>::new(
        today(),
        market.helpers.clone(),
        Actual365Fixed::new(),
        interpolator,
    )
    .unwrap();
    let kind = if upfront { "upfront" } else { "spread" };
    let oracle = if lag == 0 {
        include_str!("../../../tests/fixtures/isda_helpers/today.csv")
    } else {
        ORACLE
    };
    let rows: Vec<_> = oracle
        .lines()
        .skip(1)
        .map(|line| line.split(',').collect::<Vec<_>>())
        .filter(|row| row[0] == kind)
        .collect();
    assert_eq!(rows.len(), 9);
    for (stage, flag) in [None, Some(false), Some(true)].into_iter().enumerate() {
        market.settings.set_include_todays_cash_flows(flag);
        if stage == 1 {
            market.quotes[1].set_value(if upfront { 0.022 } else { 0.012 });
        } else if stage == 2 {
            market.rate.set_value(0.04);
        }
        for (helper, row) in market.helpers.iter().zip(&rows[stage * 3..stage * 3 + 3]) {
            assert_eq!(row[1].parse::<usize>().unwrap(), stage);
            assert_eq!(
                helper.pillar_date().serial_number(),
                row[3].parse::<i32>().unwrap()
            );
            assert_eq!(helper.latest_relevant_date(), helper.pillar_date());
            let survival = curve
                .survival_probability_date(helper.pillar_date(), false)
                .unwrap();
            let expected = row[4].parse::<f64>().unwrap();
            assert!(
                (survival - expected).abs() < 1.0e-10,
                "{kind} stage {stage}: {survival} != {expected}"
            );
            assert!(
                (helper.implied_quote().unwrap() - row[5].parse::<f64>().unwrap()).abs() < 1.0e-10
            );
            assert!(helper.quote_error().unwrap().abs() < 1.0e-10);
            assert_eq!(market.settings.include_todays_cash_flows(), flag);
        }
    }
}

#[test]
fn isda_spread_bootstraps_and_recalibrates_both_compatible_representations() {
    check_oracle::<HazardRate, _>(false, 3, BackwardFlat);
    check_oracle::<SurvivalProbability, _>(false, 3, LogLinear);
}

#[test]
fn isda_upfront_bootstraps_and_recalibrates_both_compatible_representations() {
    check_oracle::<HazardRate, _>(true, 3, BackwardFlat);
    check_oracle::<SurvivalProbability, _>(true, 3, LogLinear);
}

#[test]
fn isda_helper_failures_restore_cash_flow_settings() {
    for (valid_discount, settles_accrual, expected) in [
        (false, true, "Act/365(Fixed)"),
        (true, false, "non accrual paying CDS"),
    ] {
        let market = market(true, valid_discount, settles_accrual, 3);
        let curve = PiecewiseDefaultCurve::<HazardRate, BackwardFlat>::new(
            today(),
            market.helpers.clone(),
            Actual365Fixed::new(),
            BackwardFlat,
        )
        .unwrap();
        for flag in [None, Some(false), Some(true)] {
            market.settings.set_include_todays_cash_flows(flag);
            let error = curve.calculate().unwrap_err();
            assert!(error.message().contains(expected), "{error}");
            assert_eq!(market.settings.include_todays_cash_flows(), flag);
        }
    }
}

#[test]
fn isda_upfront_includes_todays_payment_and_restores_settings() {
    check_oracle::<HazardRate, _>(true, 0, BackwardFlat);
    check_oracle::<SurvivalProbability, _>(true, 0, LogLinear);
}
