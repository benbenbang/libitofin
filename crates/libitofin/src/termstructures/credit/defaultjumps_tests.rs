use super::*;
use crate::instrument::Instrument;
use crate::instruments::{CreditDefaultSwap, ProtectionSide};
use crate::interestrate::Compounding;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::credit::midpointcdsengine::MidPointCdsEngine;
use crate::shared::shared_mut;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::test_support::{Flag, as_observer};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Month;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;
use crate::time::schedule::MakeSchedule;

fn reference() -> Date {
    Date::new(15, Month::June, 2026)
}

fn quote(value: f64) -> Handle<dyn Quote> {
    Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
}

fn curve(jumps: Vec<Handle<dyn Quote>>, dates: Vec<Date>) -> QlResult<FlatHazardRate> {
    FlatHazardRate::with_jumps(
        reference(),
        quote(0.02),
        Actual365Fixed::new(),
        jumps,
        dates,
    )
}

#[test]
fn survival_uses_strict_boundaries_and_multiplies_only_the_elapsed_prefix() {
    let dates = vec![reference() + 100, reference() + 200];
    let curve = curve(vec![quote(0.9), quote(0.8)], dates.clone()).unwrap();
    assert_eq!(curve.jump_dates(), dates);
    assert_eq!(
        curve.jump_times().unwrap(),
        vec![100.0 / 365.0, 200.0 / 365.0]
    );
    for (days, factor) in [
        (0, 1.0),
        (99, 1.0),
        (100, 1.0),
        (101, 0.9),
        (200, 0.9),
        (201, 0.72),
    ] {
        let t = days as f64 / 365.0;
        let expected = factor * (-0.02 * t).exp();
        assert!((curve.survival_probability(t, false).unwrap() - expected).abs() < 1e-15);
        assert!(
            (curve
                .survival_probability_date(reference() + days, false)
                .unwrap()
                - expected)
                .abs()
                < 1e-15
        );
        assert!((curve.default_probability(t, false).unwrap() - (1.0 - expected)).abs() < 1e-15);
        assert!(
            (curve.default_density(t, false).unwrap() - 0.02 * (-0.02 * t).exp()).abs() < 1e-15
        );
        assert_eq!(curve.hazard_rate(t, false).unwrap(), 0.02);
    }
    let unsorted = self::curve(vec![quote(0.9), quote(0.8)], vec![dates[1], dates[0]]).unwrap();
    assert!(
        (unsorted.survival_probability(150.0 / 365.0, false).unwrap()
            - (-0.02_f64 * 150.0 / 365.0).exp())
        .abs()
            < 1e-15
    );
}

#[test]
fn invalid_jumps_are_checked_only_after_their_boundary() {
    for value in [0.0, -0.1, 1.01, f64::NAN, f64::INFINITY] {
        let curve = curve(vec![quote(value)], vec![reference() + 365]).unwrap();
        assert!(curve.survival_probability(1.0, false).is_ok());
        assert!(curve.survival_probability(1.01, false).is_err());
    }
    for handle in [
        Handle::empty(),
        Handle::new(shared(SimpleQuote::default()) as Shared<dyn Quote>),
    ] {
        let curve = curve(vec![handle], vec![reference() + 365]).unwrap();
        assert!(curve.survival_probability(1.0, false).is_ok());
        assert!(curve.survival_probability(1.01, false).is_err());
    }
    assert!(curve(vec![quote(0.9)], vec![reference(), reference() + 1]).is_err());
    assert!(curve(vec![], vec![reference()]).is_err());
    assert!(curve(vec![quote(0.9)], vec![Date::null()]).is_err());
    assert!(
        FlatHazardRate::with_jumps(
            Date::max_date(),
            quote(0.02),
            Actual365Fixed::new(),
            vec![quote(0.9), quote(0.8)],
            vec![]
        )
        .is_err()
    );
    let unity = curve(vec![quote(1.0)], vec![reference()]).unwrap();
    assert_eq!(
        unity.survival_probability(1.0, false).unwrap(),
        (-0.02_f64).exp()
    );
}

#[test]
fn generated_dates_remain_fixed_while_reference_date_and_jump_times_move() {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let curve = FlatHazardRate::moving_with_jumps(
        0,
        NullCalendar::new(),
        quote(0.02),
        Actual365Fixed::new(),
        settings.clone(),
        vec![quote(0.9), quote(0.8)],
        vec![],
    )
    .unwrap();
    let dates = [
        Date::new(31, Month::December, 2026),
        Date::new(31, Month::December, 2027),
    ];
    assert_eq!(curve.jump_dates(), dates);
    assert_eq!(
        curve.jump_times().unwrap(),
        vec![199.0 / 365.0, 564.0 / 365.0]
    );
    let flag = Flag::new();
    curve.observable().register_observer(&as_observer(&flag));
    settings.set_evaluation_date(Date::new(2, Month::January, 2027));
    assert!(Flag::is_up(&flag));
    assert_eq!(curve.jump_dates(), dates);
    assert_eq!(
        curve.jump_times().unwrap(),
        vec![-2.0 / 365.0, 363.0 / 365.0]
    );
    assert_eq!(curve.survival_probability(0.0, false).unwrap(), 0.9);
    assert!(
        (curve.survival_probability(1.0, false).unwrap() - 0.72 * (-0.02_f64).exp()).abs() < 1e-15
    );
}

#[test]
fn quote_updates_invalidate_a_cached_cds_and_dependencies_are_retained() {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let jump = shared(SimpleQuote::new(0.9));
    let probability = shared(
        curve(
            vec![Handle::new(jump.clone() as Shared<dyn Quote>)],
            vec![reference() + 100],
        )
        .unwrap(),
    );
    let discount = shared(FlatForward::with_rate(
        reference(),
        0.03,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let engine = shared_mut(MidPointCdsEngine::new(
        Handle::new(probability as Shared<dyn DefaultProbabilityTermStructure>),
        0.4,
        Handle::new(discount as Shared<dyn YieldTermStructure>),
        None,
        settings.clone(),
    ));
    let schedule = MakeSchedule::new()
        .from(reference())
        .to(reference() + 730)
        .with_frequency(Frequency::Semiannual)
        .with_calendar(NullCalendar::new())
        .with_convention(BusinessDayConvention::Unadjusted)
        .backwards()
        .build();
    let mut cds = CreditDefaultSwap::new(
        ProtectionSide::Buyer,
        1_000_000.0,
        0.01,
        schedule,
        BusinessDayConvention::Unadjusted,
        Actual365Fixed::new(),
        true,
        true,
        settings,
    )
    .unwrap();
    cds.base_mut()
        .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
    let initial = cds.npv().unwrap();
    assert_eq!(cds.npv().unwrap(), initial);
    jump.set_value(0.8);
    let bumped = cds.npv().unwrap();
    assert!(bumped > initial + 40_000.0);
    jump.set_value(0.9);
    assert!((cds.npv().unwrap() - initial).abs() < 1e-9);
}

#[test]
fn independent_quantlib_jump_oracle() {
    let first = shared(SimpleQuote::new(0.9));
    let jumps = vec![Handle::new(first.clone() as Shared<dyn Quote>), quote(0.8)];
    let fixed = curve(jumps.clone(), vec![reference() + 100, reference() + 200]).unwrap();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let moving = FlatHazardRate::moving_with_jumps(
        0,
        NullCalendar::new(),
        quote(0.02),
        Actual365Fixed::new(),
        settings.clone(),
        jumps,
        vec![],
    )
    .unwrap();
    let csv = include_str!("../../../../../sdk/go/testdata/credit_jumps_oracle.csv");
    let mut rows = 0;
    for line in csv.lines().skip(1) {
        let fields: Vec<_> = line.split(',').collect();
        let values: Vec<f64> = fields[1..]
            .iter()
            .map(|value| value.parse().unwrap())
            .collect();
        let selected = match fields[0] {
            "explicit" => &fixed,
            "quote_changed" => {
                first.set_value(0.85);
                &fixed
            }
            "moving_initial" => {
                first.set_value(0.9);
                &moving
            }
            "moving_shifted" => {
                settings.set_evaluation_date(Date::new(2, Month::January, 2027));
                &moving
            }
            scenario => panic!("unknown oracle scenario {scenario}"),
        };
        let t = values[0];
        let actual = [
            selected.survival_probability(t, false).unwrap(),
            selected.default_probability(t, false).unwrap(),
            selected.default_density(t, false).unwrap(),
            selected.hazard_rate(t, false).unwrap(),
        ];
        for (actual, expected) in actual.into_iter().zip(&values[1..]) {
            assert!(
                (actual - expected).abs() <= 1e-15,
                "{line}: {actual} != {expected}"
            );
        }
        rows += 1;
    }
    assert_eq!(rows, 12);
}
