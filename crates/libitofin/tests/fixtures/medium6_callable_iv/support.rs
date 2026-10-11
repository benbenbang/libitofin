use libitofin::errors::QlResult;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    BondPrice, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond,
};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::{FlatForward, ZeroCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::schedule::MakeSchedule;

pub fn date(text: &str) -> Date {
    let parts: Vec<i32> = text.split('-').map(|part| part.parse().unwrap()).collect();
    Date::new(parts[2], Month::from_ordinal(parts[1]), parts[0])
}

pub fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(date("2026-10-12"));
    settings
}

pub fn curve(kind: &str) -> Handle<dyn YieldTermStructure> {
    let term: Shared<dyn YieldTermStructure> = if kind == "flat" {
        shared(FlatForward::with_rate(
            date("2026-10-12"),
            0.035,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    } else {
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
    };
    Handle::new(term)
}

pub enum Product {
    Fixed(CallableFixedRateBond),
    Zero(CallableZeroCouponBond),
}

impl Product {
    pub fn instrument(&self) -> &dyn Instrument {
        match self {
            Self::Fixed(bond) => bond,
            Self::Zero(bond) => bond,
        }
    }
    pub fn instrument_mut(&mut self) -> &mut dyn Instrument {
        match self {
            Self::Fixed(bond) => bond,
            Self::Zero(bond) => bond,
        }
    }
    pub fn price(&mut self) -> f64 {
        match self {
            Self::Fixed(bond) => bond.clean_price().unwrap(),
            Self::Zero(bond) => bond.clean_price().unwrap(),
        }
    }
    pub fn iv(
        &self,
        price: BondPrice,
        curve: Handle<dyn YieldTermStructure>,
        accuracy: f64,
        budget: usize,
        min: f64,
        max: f64,
    ) -> QlResult<f64> {
        match self {
            Self::Fixed(bond) => bond.implied_volatility(price, curve, accuracy, budget, min, max),
            Self::Zero(bond) => bond.implied_volatility(price, curve, accuracy, budget, min, max),
        }
    }
}

pub fn product(
    kind: &str,
    direction: &str,
    face: f64,
    lag: u32,
    settings: Shared<Settings<Date>>,
) -> Product {
    let rights = vec![
        Callability::new(
            BondPrice::Clean(if kind == "fixed" { 100.0 } else { 85.0 }),
            if direction == "call" {
                CallabilityType::Call
            } else {
                CallabilityType::Put
            },
            date("2029-04-15"),
        )
        .unwrap(),
    ];
    product_with_rights(kind, face, lag, rights, settings)
}

pub fn product_with_rights(
    kind: &str,
    face: f64,
    lag: u32,
    rights: Vec<Callability>,
    settings: Shared<Settings<Date>>,
) -> Product {
    if kind == "fixed" {
        let schedule = MakeSchedule::new()
            .from(date("2026-01-15"))
            .to(date("2033-01-15"))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(NullCalendar::new())
            .with_convention(Bdc::Unadjusted)
            .with_termination_date_convention(Bdc::Unadjusted)
            .backwards()
            .build();
        Product::Fixed(
            CallableFixedRateBond::new(
                lag,
                face,
                schedule,
                vec![0.05],
                Actual365Fixed::new(),
                Bdc::Unadjusted,
                100.0,
                Some(date("2026-01-15")),
                rights,
                settings,
            )
            .unwrap(),
        )
    } else {
        Product::Zero(
            CallableZeroCouponBond::new(
                lag,
                face,
                NullCalendar::new(),
                date("2033-01-15"),
                Actual365Fixed::new(),
                Bdc::Unadjusted,
                100.0,
                Some(date("2026-01-15")),
                rights,
                settings,
            )
            .unwrap(),
        )
    }
}

pub fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}, tolerance {tolerance}"
    );
}
