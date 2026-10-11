use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{BondPrice, Callability, CallabilityType, CallableZeroCouponBond};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::models::shortrate::HullWhite;
use libitofin::pricingengines::bond::TreeCallableFixedRateBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::{FlatForward, ZeroCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

pub fn date(text: &str) -> Date {
    let parts: Vec<i32> = text.split('-').map(|part| part.parse().unwrap()).collect();
    Date::new(parts[2], Month::from_ordinal(parts[1]), parts[0])
}

pub fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(date("2026-10-12"));
    settings
}

pub fn curve() -> Shared<dyn YieldTermStructure> {
    shared(
        ZeroCurve::new(
            ["2026-10-12", "2027-10-12", "2030-10-12", "2035-10-12"]
                .map(date)
                .to_vec(),
            vec![0.015, 0.031, 0.024, 0.04],
            Actual365Fixed::new(),
            Linear,
        )
        .unwrap(),
    )
}

pub fn flat(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        date("2026-10-12"),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

pub fn calls(text: &str) -> Vec<Callability> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(';')
        .map(|event| {
            let parts: Vec<&str> = event.split(',').collect();
            let amount = parts[3].parse().unwrap();
            Callability::new(
                if parts[2] == "Clean" {
                    BondPrice::Clean(amount)
                } else {
                    BondPrice::Dirty(amount)
                },
                if parts[1] == "C" {
                    CallabilityType::Call
                } else {
                    CallabilityType::Put
                },
                date(parts[0]),
            )
            .unwrap()
        })
        .collect()
}

pub fn bond(
    settings: Shared<Settings<Date>>,
    calls: Vec<Callability>,
    face: f64,
    redemption: f64,
    convention: BusinessDayConvention,
) -> CallableZeroCouponBond {
    CallableZeroCouponBond::new(
        2,
        face,
        Target::new(),
        date("2031-10-12"),
        Actual365Fixed::new(),
        convention,
        redemption,
        Some(date("2026-10-09")),
        calls,
        settings,
    )
    .unwrap()
}

pub fn attach(
    bond: &mut CallableZeroCouponBond,
    curve: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
) -> SharedMut<HullWhite> {
    let model = HullWhite::new(curve, 0.073, 0.014).unwrap();
    bond.base_mut().set_pricing_engine(shared_mut(
        TreeCallableFixedRateBondEngine::new(model.clone(), 120, settings).unwrap(),
    ));
    model
}

pub fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}, tolerance {tolerance}"
    );
}

#[test]
fn checked_date_boundaries_fail_without_panics() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let settings = settings();
    let construct = |lag, maturity, convention| {
        CallableZeroCouponBond::new(
            lag,
            1000.0,
            Target::new(),
            maturity,
            Actual365Fixed::new(),
            convention,
            100.0,
            None,
            vec![],
            settings.clone(),
        )
    };
    assert!(
        catch_unwind(AssertUnwindSafe(|| construct(
            u32::MAX,
            date("2031-10-12"),
            BusinessDayConvention::Following
        )))
        .unwrap()
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| construct(
            2,
            Date::min_date(),
            BusinessDayConvention::Preceding
        )))
        .unwrap()
        .is_err()
    );
    let mut bond = construct(2, Date::max_date(), BusinessDayConvention::Unadjusted).unwrap();
    settings.reset_evaluation_date();
    assert!(
        catch_unwind(AssertUnwindSafe(|| bond.npv()))
            .unwrap()
            .is_err()
    );
    settings.set_evaluation_date(Date::null());
    assert!(
        catch_unwind(AssertUnwindSafe(|| bond.dirty_price()))
            .unwrap()
            .is_err()
    );
    settings.set_evaluation_date(Date::max_date());
    assert_eq!(
        catch_unwind(AssertUnwindSafe(|| bond.clean_price()))
            .unwrap()
            .unwrap(),
        0.0
    );
    settings.set_include_reference_date_events(true);
    let mut args = libitofin::instruments::CallableBondArguments::default();
    assert!(
        catch_unwind(AssertUnwindSafe(|| bond.setup_arguments(&mut args)))
            .unwrap()
            .is_err()
    );
}
