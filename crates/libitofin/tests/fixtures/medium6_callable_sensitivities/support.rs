use libitofin::handle::Handle;
use libitofin::instruments::{
    BondPrice, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond,
};
use libitofin::interestrate::Compounding;
use libitofin::models::shortrate::HullWhite;
use libitofin::pricingengines::bond::TreeCallableFixedRateBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::schedule::MakeSchedule;

pub fn reference() -> Date {
    Date::new(12, Month::October, 2026)
}
pub fn issue() -> Date {
    Date::new(7, Month::July, 2026)
}
pub fn maturity() -> Date {
    Date::new(7, Month::July, 2031)
}
pub fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    settings
}
pub fn engine(settings: Shared<Settings<Date>>) -> TreeCallableFixedRateBondEngine {
    let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::with_rate(
        reference(),
        0.03,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let model = HullWhite::new(Handle::new(curve), 0.1, 0.01).unwrap();
    TreeCallableFixedRateBondEngine::new(model, 100, settings).unwrap()
}
pub fn rights(right: &str) -> Vec<Callability> {
    if right == "none" {
        return vec![];
    }
    vec![
        Callability::new(
            BondPrice::Clean(100.0),
            if right == "call" {
                CallabilityType::Call
            } else {
                CallabilityType::Put
            },
            Date::new(7, Month::July, 2028),
        )
        .unwrap(),
    ]
}
pub fn fixed(settings: Shared<Settings<Date>>, calls: Vec<Callability>) -> CallableFixedRateBond {
    let schedule = MakeSchedule::new()
        .from(issue())
        .to(maturity())
        .with_frequency(Frequency::Semiannual)
        .with_calendar(NullCalendar::new())
        .with_convention(Bdc::Unadjusted)
        .with_termination_date_convention(Bdc::Unadjusted)
        .backwards()
        .build();
    CallableFixedRateBond::new(
        0,
        1000.0,
        schedule,
        vec![0.05],
        Actual365Fixed::new(),
        Bdc::Unadjusted,
        100.0,
        Some(issue()),
        calls,
        settings,
    )
    .unwrap()
}
pub fn zero(
    settings: Shared<Settings<Date>>,
    calls: Vec<Callability>,
    redemption: f64,
) -> CallableZeroCouponBond {
    CallableZeroCouponBond::new(
        0,
        1000.0,
        NullCalendar::new(),
        maturity(),
        Actual365Fixed::new(),
        Bdc::Unadjusted,
        redemption,
        Some(issue()),
        calls,
        settings,
    )
    .unwrap()
}
pub fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}, tolerance {tolerance}"
    );
}
