//! Independent QuantLib 1.43 callable-bond cases from
//! `tests/fixtures/high28/callable-prices.json` and error/lifecycle regressions.

use std::any::Any;

use libitofin::discretizedasset::DiscretizedAsset;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    BondPrice, BondResults, Callability, CallabilityType, CallableBondArguments,
    CallableFixedRateBond,
};
use libitofin::interestrate::Compounding;
use libitofin::math::array::Array;
use libitofin::models::model::CalibratedModelHolder;
use libitofin::models::shortrate::HullWhite;
use libitofin::pricingengine::{Arguments, PricingEngine};
use libitofin::pricingengines::bond::{
    DiscretizedCallableFixedRateBond, TreeCallableFixedRateBondEngine,
};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::schedule::{MakeSchedule, Schedule};

fn reference() -> Date {
    Date::new(7, Month::July, 2026)
}
fn maturity() -> Date {
    Date::new(7, Month::July, 2031)
}
fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    settings
}
fn flat(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        reference(),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
fn schedule() -> Schedule {
    MakeSchedule::new()
        .from(reference())
        .to(maturity())
        .with_frequency(Frequency::Semiannual)
        .with_calendar(NullCalendar::new())
        .with_convention(BusinessDayConvention::Unadjusted)
        .with_termination_date_convention(BusinessDayConvention::Unadjusted)
        .backwards()
        .build()
}
fn bond(
    calls: Vec<Callability>,
    settings: Shared<Settings<Date>>,
    face: f64,
) -> CallableFixedRateBond {
    CallableFixedRateBond::new(
        2,
        face,
        schedule(),
        vec![0.05],
        Actual365Fixed::new(),
        BusinessDayConvention::Unadjusted,
        100.0,
        Some(reference()),
        calls,
        settings,
    )
    .unwrap()
}
fn attach(
    bond: &mut CallableFixedRateBond,
    model: SharedMut<HullWhite>,
    settings: Shared<Settings<Date>>,
) {
    bond.base_mut().set_pricing_engine(shared_mut(
        TreeCallableFixedRateBondEngine::new(model, 100, settings).unwrap(),
    ));
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}, tolerance {tolerance}"
    );
}

#[test]
fn six_independent_quantlib_143_prices_and_quote_conventions() {
    let cases = [
        (
            None,
            Date::new(7, Month::July, 2028),
            BondPrice::Clean(100.0),
            1091.1696468618893,
            1091.3490319583746,
            109.13490319583745,
            109.1075059355635,
        ),
        (
            Some(CallabilityType::Call),
            Date::new(7, Month::July, 2028),
            BondPrice::Clean(100.0),
            1037.6283096323507,
            1037.7988926896915,
            103.77988926896914,
            103.75249200869519,
        ),
        (
            Some(CallabilityType::Put),
            Date::new(7, Month::July, 2028),
            BondPrice::Clean(100.0),
            1091.6884415430052,
            1091.8679119278177,
            109.18679119278177,
            109.15939393250781,
        ),
        (
            Some(CallabilityType::Call),
            Date::new(7, Month::August, 2028),
            BondPrice::Clean(100.0),
            1039.193208746048,
            1039.3640490682233,
            103.93640490682232,
            103.90900764654836,
        ),
        (
            Some(CallabilityType::Call),
            Date::new(7, Month::August, 2028),
            BondPrice::Dirty(100.0),
            1035.351816668888,
            1035.522025477448,
            103.5522025477448,
            103.52480528747083,
        ),
        (
            Some(CallabilityType::Call),
            Date::new(5, Month::July, 2028),
            BondPrice::Clean(100.0),
            1037.5336514697024,
            1037.7042189655183,
            103.77042189655182,
            103.74302463627785,
        ),
    ];
    for (kind, date, quote, npv, settlement, dirty, clean) in cases {
        let settings = settings();
        let model = HullWhite::new(Handle::new(flat(0.03)), 0.1, 0.01).unwrap();
        let calls = kind
            .map(|kind| vec![Callability::new(quote, kind, date).unwrap()])
            .unwrap_or_default();
        let mut bond = bond(calls, settings.clone(), 1000.0);
        attach(&mut bond, model, settings);
        close(bond.npv().unwrap(), npv, 1e-10);
        close(bond.settlement_value().unwrap(), settlement, 1e-10);
        close(bond.dirty_price().unwrap(), dirty, 1e-11);
        close(bond.clean_price().unwrap(), clean, 1e-11);
        close(
            bond.bond().accrued_amount(None).unwrap(),
            0.02739726027396472,
            1e-14,
        );
        assert_eq!(
            bond.bond().settlement_date(None).unwrap(),
            Date::new(9, Month::July, 2026)
        );
        assert_eq!(bond.valuation_date().unwrap(), reference());
    }
}

#[test]
fn values_scale_with_face_but_quotes_do_not() {
    let settings = settings();
    let call = Callability::new(
        BondPrice::Clean(100.0),
        CallabilityType::Call,
        Date::new(7, Month::August, 2028),
    )
    .unwrap();
    let model = HullWhite::new(Handle::new(flat(0.03)), 0.1, 0.01).unwrap();
    let mut small = bond(vec![call.clone()], settings.clone(), 100.0);
    let mut large = bond(vec![call], settings.clone(), 1000.0);
    attach(&mut small, model.clone(), settings.clone());
    attach(&mut large, model, settings);
    close(large.npv().unwrap(), small.npv().unwrap() * 10.0, 1e-10);
    close(
        large.clean_price().unwrap(),
        small.clean_price().unwrap(),
        1e-11,
    );
}

#[test]
fn finite_extreme_face_does_not_overflow_quote_normalization() {
    let settings = settings();
    let model = HullWhite::new(Handle::new(flat(0.03)), 0.1, 0.01).unwrap();
    let mut bond = bond(vec![], settings.clone(), 1e307);
    attach(&mut bond, model, settings);
    assert!(bond.settlement_value().unwrap().is_finite());
    close(bond.dirty_price().unwrap(), 109.13490319583745, 1e-11);
    close(bond.clean_price().unwrap(), 109.1075059355635, 1e-11);
}

#[test]
fn quote_relink_and_model_updates_invalidate_cached_prices() {
    let settings = settings();
    let quote = shared(SimpleQuote::new(0.03));
    let initial: Shared<dyn YieldTermStructure> = shared(FlatForward::new(
        reference(),
        Handle::new(quote.clone() as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let curve = RelinkableHandle::new(initial);
    let model = HullWhite::new(curve.handle().clone(), 0.1, 0.01).unwrap();
    let calls = vec![
        Callability::new(
            BondPrice::Clean(100.0),
            CallabilityType::Call,
            Date::new(7, Month::July, 2028),
        )
        .unwrap(),
    ];
    let mut bond = bond(calls.clone(), settings.clone(), 1000.0);
    attach(&mut bond, model.clone(), settings.clone());
    let initial_price = bond.clean_price().unwrap();
    assert!(bond.base().is_calculated());
    quote.set_value(0.04);
    assert!(!bond.base().is_calculated());
    let quote_price = bond.clean_price().unwrap();
    assert!((quote_price - initial_price).abs() > 0.1);
    let previous_r0 = model.borrow().r0();
    quote.set_value(f64::NAN);
    assert!(!bond.base().is_calculated());
    assert!(bond.npv().is_err());
    assert_eq!(model.borrow().r0(), previous_r0);
    quote.set_value(0.04);
    close(bond.clean_price().unwrap(), quote_price, 1e-11);
    curve.link_to(flat(0.025));
    assert!(!bond.base().is_calculated());
    let relink_price = bond.clean_price().unwrap();
    let fresh_model = HullWhite::new(Handle::new(flat(0.025)), 0.1, 0.01).unwrap();
    let mut fresh = self::bond(calls.clone(), settings.clone(), 1000.0);
    attach(&mut fresh, fresh_model, settings.clone());
    close(relink_price, fresh.clean_price().unwrap(), 1e-11);
    model
        .borrow_mut()
        .set_params(&Array::from([0.1, 0.02]))
        .unwrap();
    assert!(!bond.base().is_calculated());
    let changed_model_price = bond.clean_price().unwrap();
    assert!((changed_model_price - relink_price).abs() > 0.01);
    let fresh_model = HullWhite::new(Handle::new(flat(0.025)), 0.1, 0.02).unwrap();
    let mut fresh = self::bond(calls, settings.clone(), 1000.0);
    attach(&mut fresh, fresh_model, settings);
    close(changed_model_price, fresh.clean_price().unwrap(), 1e-11);
    let previous_r0 = model.borrow().r0();
    curve.reset();
    assert_eq!(model.borrow().r0(), previous_r0);
    assert!(!bond.base().is_calculated());
    assert!(bond.npv().is_err());
    assert!(!bond.base().is_calculated());
    curve.link_to(flat(0.025));
    assert!(!bond.base().is_calculated());
    close(model.borrow().r0(), 0.025, 1e-11);
    close(bond.clean_price().unwrap(), changed_model_price, 1e-11);
}

#[test]
fn settlement_filters_past_calls_with_types_and_expiry_is_zero() {
    let settings = settings();
    let calls = vec![
        Callability::new(BondPrice::Clean(50.0), CallabilityType::Put, reference()).unwrap(),
        Callability::new(
            BondPrice::Clean(90.0),
            CallabilityType::Put,
            Date::new(9, Month::July, 2026),
        )
        .unwrap(),
        Callability::new(
            BondPrice::Clean(100.0),
            CallabilityType::Call,
            Date::new(7, Month::July, 2028),
        )
        .unwrap(),
    ];
    let mut bond = bond(calls, settings.clone(), 1000.0);
    let mut args = CallableBondArguments::default();
    bond.setup_arguments(&mut args).unwrap();
    args.validate().unwrap();
    assert_eq!(args.callability_types, vec![CallabilityType::Call]);
    assert_eq!(args.callability_prices, vec![100.0]);
    let model = HullWhite::new(Handle::new(flat(0.03)), 0.1, 0.01).unwrap();
    attach(&mut bond, model, settings.clone());
    close(bond.npv().unwrap(), 1037.6283096323507, 1e-10);
    settings.set_evaluation_date(Date::new(8, Month::July, 2031));
    assert!(!bond.base().is_calculated());
    assert!(bond.is_expired().unwrap());
    assert_eq!(bond.npv().unwrap(), 0.0);
    assert_eq!(bond.settlement_value().unwrap(), 0.0);
    assert_eq!(bond.clean_price().unwrap(), 0.0);
}

#[test]
fn constructors_and_direct_engine_usage_return_errors_not_panics() {
    for price in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(
            Callability::new(BondPrice::Clean(price), CallabilityType::Call, reference()).is_err()
        );
    }
    assert!(
        Callability::new(BondPrice::Clean(100.0), CallabilityType::Call, Date::null()).is_err()
    );
    let settings = settings();
    for dates in [
        vec![],
        vec![reference()],
        vec![reference(), reference()],
        vec![maturity(), reference()],
        vec![Date::null(), maturity()],
    ] {
        assert!(
            CallableFixedRateBond::new(
                2,
                1000.0,
                Schedule::from_dates(dates),
                vec![0.05],
                Actual365Fixed::new(),
                BusinessDayConvention::Unadjusted,
                100.0,
                None,
                vec![],
                settings.clone()
            )
            .is_err()
        );
    }
    for face in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            CallableFixedRateBond::new(
                2,
                face,
                schedule(),
                vec![0.05],
                Actual365Fixed::new(),
                BusinessDayConvention::Unadjusted,
                100.0,
                None,
                vec![],
                settings.clone()
            )
            .is_err()
        );
    }
    let late_call = Callability::new(
        BondPrice::Clean(100.0),
        CallabilityType::Call,
        Date::new(8, Month::July, 2031),
    )
    .unwrap();
    assert!(
        CallableFixedRateBond::new(
            2,
            1000.0,
            schedule(),
            vec![0.05],
            Actual365Fixed::new(),
            BusinessDayConvention::Unadjusted,
            100.0,
            None,
            vec![late_call],
            settings.clone()
        )
        .is_err()
    );
    let model = HullWhite::new(Handle::new(flat(0.03)), 0.1, 0.01).unwrap();
    assert!(TreeCallableFixedRateBondEngine::new(model.clone(), 0, settings.clone()).is_err());
    let mut engine = TreeCallableFixedRateBondEngine::new(model, 100, settings.clone()).unwrap();
    assert!(engine.calculate().is_err());
    let curve = flat(0.03);
    assert!(
        DiscretizedCallableFixedRateBond::new(&CallableBondArguments::default(), &*curve).is_err()
    );
    let bond = bond(vec![], settings, 1000.0);
    bond.setup_arguments(engine.arguments_mut()).unwrap();
    engine.calculate().unwrap();
    let args = (engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<CallableBondArguments>()
        .unwrap();
    args.coupon_amounts.pop();
    assert!(engine.calculate().is_err());
    let results = (engine.results() as &dyn Any)
        .downcast_ref::<BondResults>()
        .unwrap();
    assert_eq!(results.instrument.value, None);
    assert_eq!(results.settlement_value, None);
}

#[test]
fn argument_validation_guards_vectors_dates_and_numerical_values() {
    let bond = bond(vec![], settings(), 1000.0);
    let mut args = CallableBondArguments::default();
    bond.setup_arguments(&mut args).unwrap();
    args.coupon_dates.swap(0, 1);
    assert!(args.validate().is_err());
    bond.setup_arguments(&mut args).unwrap();
    args.coupon_dates[0] = reference();
    assert!(args.validate().is_err());
    bond.setup_arguments(&mut args).unwrap();
    args.coupon_amounts[0] = f64::NAN;
    assert!(args.validate().is_err());
    bond.setup_arguments(&mut args).unwrap();
    args.callability_dates.push(Date::new(7, Month::July, 2028));
    assert!(args.validate().is_err());
    bond.setup_arguments(&mut args).unwrap();
    args.settlement_date = Some(Date::new(8, Month::July, 2031));
    assert!(args.validate().is_err());
    let curve = flat(0.03);
    bond.setup_arguments(&mut args).unwrap();
    args.callability_dates = vec![Date::new(7, Month::July, 2028)];
    args.callability_types = vec![CallabilityType::Call];
    args.callability_prices = vec![f64::MAX];
    assert!(DiscretizedCallableFixedRateBond::new(&args, &*curve).is_err());
    bond.setup_arguments(&mut args).unwrap();
    let discretized = DiscretizedCallableFixedRateBond::new(&args, &*curve).unwrap();
    assert_eq!(
        discretized.mandatory_times().len(),
        args.coupon_dates.len() + 1
    );
}
