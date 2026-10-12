use libitofin::handle::Handle;
use libitofin::instruments::{
    BondPrice, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond,
};
use libitofin::models::shortrate::HullWhite;
use libitofin::pricingengines::bond::TreeCallableFixedRateBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::DiscountCurve;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::schedule::MakeSchedule;

pub fn date(day: i32, month: Month, year: i32) -> Date {
    Date::new(day, month, year)
}
pub fn reference() -> Date {
    date(7, Month::July, 2026)
}
pub fn maturity() -> Date {
    date(7, Month::July, 2031)
}
pub fn settings() -> Shared<Settings<Date>> {
    let s = shared(Settings::new());
    s.set_evaluation_date(reference());
    s
}
pub fn curve() -> Shared<dyn YieldTermStructure> {
    shared(
        DiscountCurve::new(
            vec![
                reference(),
                date(7, Month::July, 2027),
                date(7, Month::July, 2029),
                date(7, Month::July, 2036),
            ],
            vec![1.0, 0.983, 0.915, 0.67],
            Actual365Fixed::new(),
            Some(NullCalendar::new()),
        )
        .unwrap(),
    )
}
pub fn engine(s: Shared<Settings<Date>>) -> TreeCallableFixedRateBondEngine {
    TreeCallableFixedRateBondEngine::new(
        HullWhite::new(Handle::new(curve()), 0.1, 0.01).unwrap(),
        100,
        s,
    )
    .unwrap()
}
pub fn calls() -> Vec<Callability> {
    vec![
        Callability::new(
            BondPrice::Clean(100.0),
            CallabilityType::Call,
            date(7, Month::August, 2028),
        )
        .unwrap(),
        Callability::new(
            BondPrice::Dirty(97.0),
            CallabilityType::Put,
            date(7, Month::January, 2029),
        )
        .unwrap(),
    ]
}
pub fn fixed(
    lag: u32,
    s: Shared<Settings<Date>>,
    calls: Vec<Callability>,
) -> CallableFixedRateBond {
    fixed_face(lag, 12345.0, s, calls)
}

pub fn fixed_face(
    lag: u32,
    face: f64,
    s: Shared<Settings<Date>>,
    calls: Vec<Callability>,
) -> CallableFixedRateBond {
    let schedule = MakeSchedule::new()
        .from(reference())
        .to(maturity())
        .with_frequency(Frequency::Semiannual)
        .with_calendar(NullCalendar::new())
        .with_convention(Bdc::Unadjusted)
        .with_termination_date_convention(Bdc::Unadjusted)
        .backwards()
        .build();
    CallableFixedRateBond::new(
        lag,
        face,
        schedule,
        vec![0.05],
        Actual365Fixed::new(),
        Bdc::Unadjusted,
        100.0,
        Some(reference()),
        calls,
        s,
    )
    .unwrap()
}
pub fn zero(
    lag: u32,
    s: Shared<Settings<Date>>,
    calls: Vec<Callability>,
) -> CallableZeroCouponBond {
    zero_face(lag, 12345.0, s, calls)
}

pub fn zero_face(
    lag: u32,
    face: f64,
    s: Shared<Settings<Date>>,
    calls: Vec<Callability>,
) -> CallableZeroCouponBond {
    CallableZeroCouponBond::new(
        lag,
        face,
        NullCalendar::new(),
        maturity(),
        Actual365Fixed::new(),
        Bdc::Unadjusted,
        100.0,
        Some(reference()),
        calls,
        s,
    )
    .unwrap()
}
pub fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}, tolerance {tolerance}"
    );
}

pub struct UpdateCounter(pub std::rc::Rc<std::cell::Cell<usize>>);

impl libitofin::patterns::observable::Observer for UpdateCounter {
    fn update(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
