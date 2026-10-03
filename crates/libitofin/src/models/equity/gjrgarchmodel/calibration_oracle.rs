//! Original QuantLib DAX calibration case and independent native endpoint.
//!
//! Inputs port `test-suite/gjrgarchmodel.cpp` at commit
//! `9863b578af0caa4cecabf697196533e84a8308b6` (Copyright 2008 Yee Man Chan,
//! QuantLib license). Endpoint fixtures were independently generated with
//! native QuantLib 1.43, not the implementation under test.

use super::*;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::math::distributions::normal::CumulativeNormalDistribution;
use crate::math::interpolations::linear::Linear;
use crate::math::optimization::{endcriteria::EndCriteria, simplex::Simplex};
use crate::models::calibrate;
use crate::models::calibrationhelper::{
    BlackCalibrationHelper, CalibrationErrorType, CalibrationHelper,
};
use crate::models::equity::HestonModelHelper;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::vanilla::AnalyticGjrGarchEngine;
use crate::quotes::make_quote_handle;
use crate::settings::Settings;
use crate::termstructures::yields::{FlatForward, ZeroCurve};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::{
    businessdayconvention::BusinessDayConvention,
    calendars::Target,
    date::{Date, Month},
    daycounters::actual365fixed::Actual365Fixed,
    frequency::Frequency,
    period::Period,
    timeunit::TimeUnit,
};

const SPOT: Real = 4468.17;
const NATIVE_PARAMS: [Real; 6] = [
    8.898622374140302e-06,
    0.006591983104938795,
    0.872596854986186,
    0.08607105571208255,
    0.4687141475249288,
    0.0004236245700445744,
];
const NATIVE_SSE: Real = 7.852200556326041;
#[rustfmt::skip]
const ROWS: [[Real; 5]; 21] = [
    [4000.0, 0.4541, 18.602404320586533, 16.979486227079178, -0.011102381828032803],
    [4000.0, 0.3869, 58.24307057009883, 61.48238198897616, 0.008263643129223508],
    [4000.0, 0.3492, 90.77210042034709, 95.15759238965438, 0.007426196818510922],
    [4200.0, 0.406, 41.67897544091126, 41.01500783965639, -0.0027017402594170514],
    [4200.0, 0.3607, 98.36441992292276, 96.88080765612631, -0.00294064905094249],
    [4200.0, 0.333, 140.47991834739616, 137.39211018843707, -0.0043417992782068815],
    [4400.0, 0.3726, 95.25352776496862, 94.24615254433684, -0.0029900582445397395],
    [4400.0, 0.3396, 163.23413921647207, 157.85761219327742, -0.00912463192509455],
    [4400.0, 0.3108, 204.66952324243343, 202.79215676605418, -0.002354410573404575],
    [4500.0, 0.355, 111.84674426382676, 112.83803850964989, 0.002842685402450973],
    [4500.0, 0.3277, 191.5270494876106, 187.26146110571503, -0.007056972888980828],
    [4500.0, 0.3012, 246.1639434528661, 245.9138869478238, -0.00030616786542392305],
    [4600.0, 0.3428, 68.7470006034586, 70.79298129079505, 0.006287818146115898],
    [4600.0, 0.3209, 144.8786811453041, 140.1154284840995, -0.008005294121785134],
    [4600.0, 0.2958, 198.41557942276972, 197.54989769137345, -0.0010619844720531346],
    [4800.0, 0.3302, 21.079674939379927, 22.81902028120816, 0.008556468457676825],
    [4800.0, 0.3062, 74.40272324961417, 72.48433413425394, -0.003806388010797246],
    [4800.0, 0.2799, 116.9918055370642, 121.42246686738152, 0.005938449118704725],
    [5000.0, 0.3343, 5.697780522868121, 6.008297248660749, 0.003498842692311821],
    [5000.0, 0.2959, 33.74042694387325, 34.366661711576675, 0.0017546868312284802],
    [5000.0, 0.2705, 64.59595626953389, 70.84037518752511, 0.010245385271263618],
];

struct DaxCase {
    model: SharedMut<GjrGarchModel>,
    helpers: Vec<SharedMut<HestonModelHelper>>,
}

fn dax_case() -> DaxCase {
    let date = Date::new(5, Month::July, 2002);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(date);
    let day_counter = Actual365Fixed::new();
    let days = [0, 13, 41, 75, 165, 256, 345, 524, 703];
    let rates = vec![
        0.0357, 0.0357, 0.0349, 0.0341, 0.0355, 0.0359, 0.0368, 0.0386, 0.0401,
    ];
    let risk_free: Handle<dyn YieldTermStructure> = Handle::new(shared(
        ZeroCurve::new(
            days.iter().map(|&days| date + days).collect(),
            rates,
            day_counter.clone(),
            Linear,
        )
        .unwrap(),
    ));
    let dividend: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
        date,
        0.0,
        day_counter,
        Compounding::Continuous,
        Frequency::Annual,
    )));
    let lambda = 0.1;
    let alpha = 0.024;
    let beta = 0.93;
    let gamma = 0.059;
    let omega = 2e-6;
    let m1 = beta
        + (alpha + gamma * CumulativeNormalDistribution::standard().value(lambda))
            * (1.0 + lambda * lambda)
        + gamma * lambda * (-lambda * lambda / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let v0 = omega / (1.0 - m1);
    assert!((v0 - 0.00017778348033623922).abs() < 1e-18);
    let process = shared(
        GjrGarchProcess::new(
            risk_free.clone(),
            dividend.clone(),
            make_quote_handle(SPOT).handle(),
            GjrGarchParameters {
                omega,
                alpha,
                beta,
                gamma,
                lambda,
                v0,
                days_per_year: 365.0,
            },
            GjrGarchDiscretization::FullTruncation,
        )
        .unwrap(),
    );
    let model = GjrGarchModel::new(process).unwrap();
    let engine = shared_mut(AnalyticGjrGarchEngine::new(SharedMut::clone(&model)))
        as SharedMut<dyn PricingEngine>;
    let calendar = Target::new();
    let mut helpers = Vec::new();
    for (index, row) in ROWS.iter().enumerate() {
        let term = index % 3;
        let maturity = Period::new([2, 6, 11][term], TimeUnit::Weeks);
        let expiry =
            calendar.advance_by_period(date, maturity, BusinessDayConvention::Following, false);
        assert_eq!(expiry, date + [14, 42, 77][term]);
        let helper = shared_mut(HestonModelHelper::new(
            maturity,
            calendar.clone(),
            SPOT,
            row[0],
            make_quote_handle(row[1]).handle(),
            risk_free.clone(),
            dividend.clone(),
            CalibrationErrorType::ImpliedVolError,
            Shared::clone(&settings),
        ));
        helper
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(SharedMut::clone(&engine));
        assert!(
            (helper.borrow().maturity().unwrap() - [14.0, 42.0, 77.0][term] / 365.0).abs() < 1e-15
        );
        let expected_discount = [0.9986327164070337, 0.9959948591398816, 0.9928255979859668][term];
        let discount = risk_free
            .current_link()
            .unwrap()
            .discount_date(expiry, false)
            .unwrap();
        assert!((discount - expected_discount).abs() < 1e-15);
        let market = helper.borrow_mut().market_value().unwrap();
        assert!(
            (market - row[2]).abs() < 1e-10,
            "market input row {index}: {market} versus {}",
            row[2]
        );
        helpers.push(helper);
    }
    DaxCase { model, helpers }
}

fn percent_vol_sse(helpers: &[SharedMut<HestonModelHelper>]) -> Real {
    helpers
        .iter()
        .map(|helper| (helper.borrow_mut().calibration_error().unwrap() * 100.0).powi(2))
        .sum()
}

#[test]
fn original_dax_native_endpoint_matches_prices_and_implied_vol_sse() {
    let case = dax_case();
    case.model
        .borrow_mut()
        .set_params(&Array::from(NATIVE_PARAMS))
        .unwrap();
    for (index, (helper, row)) in case.helpers.iter().zip(ROWS).enumerate() {
        let price = helper.borrow().model_value().unwrap();
        assert!(
            (price - row[3]).abs() < 1e-10,
            "native endpoint row {index}: {price} versus {}",
            row[3]
        );
        let residual = helper.borrow_mut().calibration_error().unwrap();
        assert!((residual - row[4]).abs() < 1e-11);
    }
    let sse = percent_vol_sse(&case.helpers);
    assert!(
        (sse - NATIVE_SSE).abs() < 1e-8,
        "native SSE {sse} versus {NATIVE_SSE}"
    );
}

#[test]
fn original_dax_21_helper_calibration_preserves_quantlib_solver_and_acceptance() {
    let case = dax_case();
    let instruments: Vec<SharedMut<dyn CalibrationHelper>> = case
        .helpers
        .iter()
        .map(|helper| SharedMut::clone(helper) as SharedMut<dyn CalibrationHelper>)
        .collect();
    let mut method = Simplex::new(0.05);
    let criteria = EndCriteria::new(400, Some(40), 1e-8, 1e-8, Some(1e-8)).unwrap();
    calibrate(
        &case.model,
        &instruments,
        &mut method,
        &criteria,
        None,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let sse = percent_vol_sse(&case.helpers);
    assert!(
        (sse - NATIVE_SSE).abs() < 1e-6,
        "fitted SSE {sse} versus {NATIVE_SSE}"
    );
    for (index, (helper, row)) in case.helpers.iter().zip(ROWS).enumerate() {
        let price = helper.borrow().model_value().unwrap();
        assert!(
            (price - row[3]).abs() < 1e-5,
            "fitted price row {index}: {price} versus {}",
            row[3]
        );
    }
    let model = case.model.borrow();
    assert!(
        sse <= 15.0,
        "original DAX percent-vol SSE {sse}, native endpoint SSE {NATIVE_SSE}; parameters {:?}",
        model.calibrated_model().params()
    );
    assert!(model.constraint().test(&model.calibrated_model().params()));
    assert_eq!(model.process().days_per_year(), 365.0);
    assert_eq!(model.calibrated_model().problem_values().size(), 21);
    assert!(model.calibrated_model().function_evaluation() > 0);
    eprintln!(
        "original DAX calibration: SSE {sse:.16}, native SSE {NATIVE_SSE:.16}, params {:?}, evaluations {}",
        model.calibrated_model().params(),
        model.calibrated_model().function_evaluation()
    );
}
