use super::*;
use crate::handle::RelinkableHandle;
use crate::instrument::Instrument;
use crate::instruments::{BondPrice, Callability, CallableFixedRateBond, CallableZeroCouponBond};
use crate::quotes::SimpleQuote;
use crate::shared::{SharedMut, shared, shared_mut};
use crate::termstructures::yields::FlatForward;
use crate::time::businessdayconvention::BusinessDayConvention::Unadjusted;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Month::January;
use crate::time::schedule::MakeSchedule;

fn today() -> Date {
    Date::new(15, January, 2026)
}
fn exercise() -> Date {
    Date::new(15, January, 2028)
}
fn maturity() -> Date {
    Date::new(15, January, 2031)
}
fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today());
    settings
}
fn curve(rate: Real) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        today(),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
fn right(put: bool, date: Date) -> Callability {
    Callability::new(
        BondPrice::Clean(100.0),
        if put {
            CallabilityType::Put
        } else {
            CallabilityType::Call
        },
        date,
    )
    .unwrap()
}
fn fixed(lag: u32, put: bool, settings: Shared<Settings<Date>>) -> CallableFixedRateBond {
    let schedule = MakeSchedule::new()
        .from(today())
        .to(maturity())
        .with_frequency(Frequency::Semiannual)
        .with_calendar(NullCalendar::new())
        .with_convention(Unadjusted)
        .with_termination_date_convention(Unadjusted)
        .backwards()
        .build();
    CallableFixedRateBond::new(
        lag,
        1000.0,
        schedule,
        vec![0.055],
        Actual365Fixed::new(),
        Unadjusted,
        100.0,
        Some(today()),
        vec![right(put, exercise())],
        settings,
    )
    .unwrap()
}
fn zero(lag: u32, put: bool, settings: Shared<Settings<Date>>) -> CallableZeroCouponBond {
    CallableZeroCouponBond::new(
        lag,
        1000.0,
        NullCalendar::new(),
        maturity(),
        Actual365Fixed::new(),
        Unadjusted,
        100.0,
        Some(today()),
        vec![right(put, exercise())],
        settings,
    )
    .unwrap()
}
fn engine(volatility: Real, settings: Shared<Settings<Date>>) -> SharedMut<dyn PricingEngine> {
    shared_mut(BlackCallableFixedRateBondEngine::new(
        Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>),
        Handle::new(curve(0.04)),
        settings,
    ))
}
fn near(actual: Real, expected: Real) {
    assert!(
        (actual - expected).abs() < 1.0e-8,
        "actual {actual:.15} expected {expected:.15}"
    );
}

#[test]
fn native_reference_parity_and_independent_settlement_normalization() {
    for line in include_str!("black_callable_oracle.csv").lines().skip(1) {
        let fields: Vec<_> = line.split(',').collect();
        let is_zero: bool = fields[0].parse().unwrap();
        let put: bool = fields[1].parse().unwrap();
        let lag = fields[2].parse().unwrap();
        let volatility = fields[3].parse().unwrap();
        let npv = fields[4].parse().unwrap();
        let settlement = fields[5].parse().unwrap();
        let settings = settings();
        if is_zero {
            let mut bond = zero(lag, put, Shared::clone(&settings));
            bond.base_mut()
                .set_pricing_engine(engine(volatility, settings));
            near(bond.npv().unwrap(), npv);
            near(bond.settlement_value().unwrap(), settlement);
            near(bond.clean_price().unwrap(), settlement / 10.0);
        } else {
            let mut bond = fixed(lag, put, Shared::clone(&settings));
            bond.base_mut()
                .set_pricing_engine(engine(volatility, settings));
            near(bond.npv().unwrap(), npv);
            near(bond.settlement_value().unwrap(), settlement);
            near(bond.dirty_price().unwrap(), settlement / 10.0);
        }
    }
}

#[test]
fn quotes_curves_and_evaluation_dates_invalidate_without_breaking_freeze() {
    let settings = settings();
    let quote = shared(SimpleQuote::new(0.10));
    let discount = RelinkableHandle::new(curve(0.04));
    let pricing = shared_mut(BlackCallableFixedRateBondEngine::new(
        Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
        discount.handle(),
        Shared::clone(&settings),
    ));
    let mut bond = fixed(3, false, Shared::clone(&settings));
    bond.base_mut()
        .set_pricing_engine(pricing as SharedMut<dyn PricingEngine>);
    let original = bond.npv().unwrap();
    quote.set_value(0.35);
    assert!(!bond.base().is_calculated());
    let volatile = bond.npv().unwrap();
    assert!(volatile < original);
    bond.base_mut().freeze();
    quote.set_value(0.20);
    assert_eq!(bond.npv().unwrap(), volatile);
    bond.base_mut().unfreeze();
    assert!(bond.npv().unwrap() > volatile);
    discount.link_to(curve(0.05));
    assert!(!bond.base().is_calculated());
    let relinked = bond.npv().unwrap();
    settings.set_evaluation_date(today() + 30);
    assert!(!bond.base().is_calculated());
    assert_ne!(bond.npv().unwrap(), relinked);
}

#[test]
fn calculation_failures_reset_results_and_recover() {
    let settings = settings();
    let quote = shared(SimpleQuote::new(0.10));
    let mut pricing = BlackCallableFixedRateBondEngine::new(
        Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
        Handle::new(curve(0.04)),
        Shared::clone(&settings),
    );
    let bond = fixed(3, false, settings);
    bond.setup_arguments(pricing.arguments_mut()).unwrap();
    pricing.calculate().unwrap();
    assert!(
        pricing
            .results()
            .as_instrument_results()
            .unwrap()
            .value
            .is_some()
    );
    for invalid in [-0.1, f64::NAN, f64::INFINITY] {
        quote.set_value(invalid);
        assert!(pricing.calculate().is_err());
        assert!(
            pricing
                .results()
                .as_instrument_results()
                .unwrap()
                .value
                .is_none()
        );
    }
    quote.set_value(0.1);
    pricing.calculate().unwrap();
    assert!(
        pricing
            .results()
            .as_instrument_results()
            .unwrap()
            .value
            .is_some()
    );
}

#[test]
fn malformed_direct_arguments_and_missing_handles_fail_without_panicking() {
    let settings = settings();
    let bond = zero(0, false, Shared::clone(&settings));
    let mut pricing =
        BlackCallableFixedRateBondEngine::new(Handle::empty(), Handle::empty(), settings);
    assert!(pricing.calculate().is_err());
    bond.setup_arguments(pricing.arguments_mut()).unwrap();
    assert!(pricing.calculate().is_err());
    let args = pricing.base.arguments_mut();
    args.callability_count = 2;
    assert!(args.validate().is_err());
    args.callability_count = 1;
    args.bond.callability_dates.clear();
    assert!(args.validate().is_err());
}

#[test]
fn expired_and_multiple_original_rights_do_not_become_european() {
    let settings = settings();
    let mut pricing = BlackCallableFixedRateBondEngine::new(
        Handle::new(shared(SimpleQuote::new(0.1)) as Shared<dyn Quote>),
        Handle::new(curve(0.04)),
        Shared::clone(&settings),
    );
    let bond = CallableZeroCouponBond::new(
        0,
        1000.0,
        NullCalendar::new(),
        maturity(),
        Actual365Fixed::new(),
        Unadjusted,
        100.0,
        Some(today()),
        vec![right(true, today() - 1), right(false, exercise())],
        Shared::clone(&settings),
    )
    .unwrap();
    bond.setup_arguments(pricing.arguments_mut()).unwrap();
    assert!(pricing.calculate().is_err());
    let bond = CallableZeroCouponBond::new(
        0,
        1000.0,
        NullCalendar::new(),
        maturity(),
        Actual365Fixed::new(),
        Unadjusted,
        100.0,
        Some(today()),
        vec![right(true, maturity())],
        Shared::clone(&settings),
    )
    .unwrap();
    bond.setup_arguments(pricing.arguments_mut()).unwrap();
    assert!(pricing.calculate().is_err());
    settings.set_evaluation_date(exercise() + 1);
    let bond = zero(0, false, Shared::clone(&settings));
    bond.setup_arguments(pricing.arguments_mut()).unwrap();
    assert!(pricing.calculate().is_err());
}

#[test]
fn existing_callable_argument_struct_literals_remain_source_compatible() {
    let arguments = CallableBondArguments {
        settlement_date: Some(today()),
        coupon_dates: Vec::new(),
        coupon_amounts: Vec::new(),
        face_amount: 1000.0,
        redemption: 1000.0,
        redemption_date: Some(maturity()),
        callability_prices: vec![100.0],
        callability_types: vec![CallabilityType::Call],
        callability_dates: vec![exercise()],
    };
    arguments.validate().unwrap();
}

#[test]
fn ex_coupon_income_subtraction_respects_ownership_at_settlement() {
    let settings = settings();
    let evaluation = Date::new(10, January, 2026);
    settings.set_evaluation_date(evaluation);
    let schedule = MakeSchedule::new()
        .from(Date::new(15, January, 2025))
        .to(maturity())
        .with_frequency(Frequency::Semiannual)
        .with_calendar(NullCalendar::new())
        .with_convention(Unadjusted)
        .with_termination_date_convention(Unadjusted)
        .backwards()
        .build();
    let mut bond = CallableFixedRateBond::with_ex_coupon(
        0,
        1000.0,
        schedule,
        vec![0.055],
        Actual365Fixed::new(),
        Unadjusted,
        100.0,
        Some(Date::new(15, January, 2025)),
        vec![right(false, exercise())],
        Some(crate::time::period::Period::new(
            10,
            crate::time::timeunit::TimeUnit::Days,
        )),
        NullCalendar::new(),
        Unadjusted,
        false,
        Shared::clone(&settings),
    )
    .unwrap();
    let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::with_rate(
        evaluation,
        0.04,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let pricing = shared_mut(BlackCallableFixedRateBondEngine::new(
        Handle::new(shared(SimpleQuote::new(0.2)) as Shared<dyn Quote>),
        Handle::new(curve),
        settings,
    ));
    bond.base_mut()
        .set_pricing_engine(pricing as SharedMut<dyn PricingEngine>);
    near(bond.npv().unwrap(), 1025.7631785714432);
}

#[test]
fn fixed_quote_and_setup_guard_settlement_date_overflow() {
    let settings = settings();
    let bond = fixed(3, false, Shared::clone(&settings));
    let mut pricing = BlackCallableFixedRateBondEngine::new(
        Handle::new(shared(SimpleQuote::new(0.1)) as Shared<dyn Quote>),
        Handle::new(curve(0.04)),
        Shared::clone(&settings),
    );
    settings.set_evaluation_date(Date::max_date());
    assert!(bond.setup_arguments(pricing.arguments_mut()).is_err());
    let mut bond = bond;
    assert!(bond.dirty_price().is_err());
    assert!(bond.clean_price().is_err());
}

#[test]
fn forward_yield_and_prices_are_scale_invariant_for_tiny_face() {
    for face in [1.0e-12, 1.0e-25, 1.0e-100] {
        let settings = settings();
        let mut bond = CallableZeroCouponBond::new(
            3,
            face,
            NullCalendar::new(),
            maturity(),
            Actual365Fixed::new(),
            Unadjusted,
            100.0,
            Some(today()),
            vec![right(true, exercise())],
            Shared::clone(&settings),
        )
        .unwrap();
        bond.base_mut()
            .set_pricing_engine(engine(0.35, Shared::clone(&settings)));
        let price = bond.dirty_price().unwrap();
        let mut reference = zero(3, true, Shared::clone(&settings));
        reference
            .base_mut()
            .set_pricing_engine(engine(0.35, settings));
        assert!((price - reference.dirty_price().unwrap()).abs() < 1.0e-10);
    }
}

#[test]
fn exactly_zero_forward_yield_prices_intrinsic_without_rounding_negative() {
    let evaluation = Date::new(12, crate::time::date::Month::October, 2026);
    for put in [false, true] {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(evaluation);
        let right = Callability::new(
            BondPrice::Clean(97.25),
            if put {
                CallabilityType::Put
            } else {
                CallabilityType::Call
            },
            evaluation + 18,
        )
        .unwrap();
        let mut bond = CallableZeroCouponBond::new(
            17,
            12345.0,
            NullCalendar::new(),
            Date::new(12, crate::time::date::Month::October, 2027),
            Actual365Fixed::new(),
            Unadjusted,
            107.5,
            Some(evaluation),
            vec![right],
            Shared::clone(&settings),
        )
        .unwrap();
        let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::with_rate(
            evaluation,
            0.0,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        let pricing = shared_mut(BlackCallableFixedRateBondEngine::new(
            Handle::new(shared(SimpleQuote::new(0.23)) as Shared<dyn Quote>),
            Handle::new(curve),
            settings,
        ));
        bond.base_mut()
            .set_pricing_engine(pricing as SharedMut<dyn PricingEngine>);
        near(bond.dirty_price().unwrap(), if put { 107.5 } else { 97.25 });
    }
}
