//! QuantLib `defaultprobabilitycurves.cpp:326-335`: both density conventions,
//! independently rebuilt spread/upfront contracts, unchanged 1e-6 tolerance.

use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{CdsTerms, CreditDefaultSwap, ProtectionSide, cds_maturity};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::{Interpolator, flat::BackwardFlat, linear::Linear};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::credit::MidPointCdsEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::credit::defaultprobabilityhelpers::{
    CdsHelperTerms, DefaultProbabilityHelper, SpreadCdsHelper, UpfrontCdsHelper,
};
use libitofin::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use libitofin::termstructures::credit::piecewisedefaultcurve::PiecewiseDefaultCurve;
use libitofin::termstructures::credit::probabilitytraits::DefaultDensity;
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::dategenerationrule::DateGeneration;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::{
    actual360::Actual360,
    thirty360::{Convention, Thirty360},
};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::Schedule;
use libitofin::time::timeunit::TimeUnit;

const TOLERANCE: f64 = 1e-6;
fn day_counter() -> DayCounter {
    Thirty360::with_convention(Convention::BondBasis)
}

fn density_round_trips<I: Interpolator + 'static>(interpolator: I, upfront: bool) {
    let today = Date::new(9, Month::June, 2006);
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);
    settings.set_include_todays_cash_flows(Some(true));
    let discount = Handle::new(shared(FlatForward::with_rate(
        today,
        0.06,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    let quotes = if upfront {
        [0.01, 0.02, 0.04, 0.06]
    } else {
        [0.005, 0.006, 0.007, 0.009]
    };
    let tenors = if upfront { [2, 3, 5, 7] } else { [1, 2, 3, 5] };
    let live_quotes: Vec<_> = quotes
        .iter()
        .map(|value| shared(SimpleQuote::new(*value)))
        .collect();
    let helpers: Vec<Shared<dyn DefaultProbabilityHelper>> = live_quotes
        .iter()
        .zip(tenors)
        .map(|(quote, years)| {
            let quote = Handle::new(Shared::clone(quote) as Shared<dyn Quote>);
            let tenor = Period::new(years, TimeUnit::Years);
            if upfront {
                UpfrontCdsHelper::with_terms(
                    quote,
                    0.05,
                    tenor,
                    1,
                    Target::new(),
                    Frequency::Quarterly,
                    Bdc::ModifiedFollowing,
                    DateGeneration::CDS,
                    Actual360::new(),
                    0.4,
                    discount.clone(),
                    3,
                    CdsHelperTerms {
                        last_period_day_counter: Some(Actual360::with_last_day(true)),
                        ..CdsHelperTerms::default()
                    },
                    Shared::clone(&settings),
                )
                .unwrap() as Shared<dyn DefaultProbabilityHelper>
            } else {
                SpreadCdsHelper::new(
                    quote,
                    tenor,
                    1,
                    Target::new(),
                    Frequency::Quarterly,
                    Bdc::Following,
                    DateGeneration::TwentiethIMM,
                    day_counter(),
                    0.4,
                    discount.clone(),
                    Shared::clone(&settings),
                )
                .unwrap() as Shared<dyn DefaultProbabilityHelper>
            }
        })
        .collect();
    let curve = PiecewiseDefaultCurve::<DefaultDensity, I>::new(
        today,
        helpers.clone(),
        day_counter(),
        interpolator,
    )
    .unwrap();
    let curve_handle =
        Handle::new(Shared::clone(&curve) as Shared<dyn DefaultProbabilityTermStructure>);
    for (quote, years) in quotes.iter().zip(tenors) {
        let tenor = Period::new(years, TimeUnit::Years);
        let (start, end, convention, rule, dc) = if upfront {
            (
                today + 1,
                cds_maturity(today, tenor, DateGeneration::CDS)
                    .unwrap()
                    .unwrap(),
                Bdc::ModifiedFollowing,
                DateGeneration::CDS,
                Actual360::new(),
            )
        } else {
            (
                Target::new().adjust(today + 1, Bdc::Following),
                today + tenor,
                Bdc::Following,
                DateGeneration::TwentiethIMM,
                day_counter(),
            )
        };
        let schedule = Schedule::new(
            start,
            end,
            Period::try_from(Frequency::Quarterly).unwrap(),
            Target::new(),
            convention,
            Bdc::Unadjusted,
            rule,
            false,
            Date::null(),
            Date::null(),
        );
        let mut cds = if upfront {
            CreditDefaultSwap::with_upfront_and_terms(
                ProtectionSide::Buyer,
                1.0,
                *quote,
                0.05,
                schedule,
                convention,
                dc,
                CdsTerms {
                    protection_start: Some(today + 1),
                    upfront_date: Some(Target::new().advance(
                        today,
                        3,
                        TimeUnit::Days,
                        Bdc::ModifiedFollowing,
                        false,
                    )),
                    last_period_day_counter: Some(Actual360::with_last_day(true)),
                    trade_date: Some(today),
                    ..CdsTerms::default()
                },
                Shared::clone(&settings),
            )
            .unwrap()
        } else {
            CreditDefaultSwap::with_terms(
                ProtectionSide::Buyer,
                1.0,
                *quote,
                schedule,
                convention,
                dc,
                CdsTerms {
                    protection_start: Some(today + 1),
                    ..CdsTerms::default()
                },
                Shared::clone(&settings),
            )
            .unwrap()
        };
        cds.base_mut()
            .set_pricing_engine(shared_mut(MidPointCdsEngine::new(
                curve_handle.clone(),
                0.4,
                discount.clone(),
                Some(true),
                Shared::clone(&settings),
            )) as SharedMut<dyn PricingEngine>);
        let actual = if upfront {
            cds.fair_upfront().unwrap()
        } else {
            cds.fair_spread().unwrap()
        };
        assert!(
            (actual - quote).abs() <= TOLERANCE,
            "{years}Y upfront={upfront}: {actual} vs {quote}"
        );
    }
    let initial = curve.survival_probability(1.0, false).unwrap();
    live_quotes[0].set_value(quotes[0] + 0.001);
    let updated = curve.survival_probability(1.0, false).unwrap();
    assert!(updated < initial);
    for (i, helper) in helpers.iter().enumerate() {
        let expected = quotes[i] + if i == 0 { 0.001 } else { 0.0 };
        assert!((helper.implied_quote().unwrap() - expected).abs() <= TOLERANCE);
    }
    let data = curve.data().unwrap();
    assert_eq!(data[0], data[1]);
    assert!(data.iter().all(|density| *density >= 0.0));
    live_quotes[0].set_value(-1.0);
    assert!(curve.calculate().is_err());
    live_quotes[0].set_value(quotes[0]);
    curve.calculate().unwrap();
    assert!((helpers[0].implied_quote().unwrap() - quotes[0]).abs() <= TOLERANCE);
}

#[test]
fn default_density_backward_flat_spread_round_trip() {
    density_round_trips(BackwardFlat, false);
}
#[test]
fn default_density_linear_spread_round_trip() {
    density_round_trips(Linear, false);
}
#[test]
fn default_density_backward_flat_upfront_round_trip() {
    density_round_trips(BackwardFlat, true);
}
#[test]
fn default_density_linear_upfront_round_trip() {
    density_round_trips(Linear, true);
}
