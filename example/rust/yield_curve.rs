//! Bootstrap a `PiecewiseYieldCurve` from deposit + swap rate helpers, then
//! query discount factors and zero rates off the solved curve.
//!
//! The market strip is transcribed from QuantLib's piecewiseyieldcurve.cpp.
//! D5: the evaluation date is set explicitly on an owned `Settings` (an unset
//! date is an Err, not a system-clock fallback).

use libitofin::handle::Handle;
use libitofin::indexes::ibor::euribor::Euribor;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::loglinear::LogLinear;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::bootstraphelper::RateHelper;
use libitofin::termstructures::bootstraptraits::Discount;
use libitofin::termstructures::yields::{DepositRateHelper, PiecewiseYieldCurve, SwapRateHelper};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::thirty360::{Convention, Thirty360};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use libitofin::types::Rate;

// Market strip transcribed from QuantLib's piecewiseyieldcurve.cpp
// (deposits :364-378, swaps :379-403). (n, unit, rate-in-percent).
const DEPOSIT_DATA: [(i32, TimeUnit, f64); 6] = [
    (1, TimeUnit::Weeks, 4.559),
    (1, TimeUnit::Months, 4.581),
    (2, TimeUnit::Months, 4.573),
    (3, TimeUnit::Months, 4.557),
    (6, TimeUnit::Months, 4.496),
    (9, TimeUnit::Months, 4.490),
];

const SWAP_DATA: [(i32, TimeUnit, f64); 15] = [
    (1, TimeUnit::Years, 4.54),
    (2, TimeUnit::Years, 4.63),
    (3, TimeUnit::Years, 4.75),
    (4, TimeUnit::Years, 4.86),
    (5, TimeUnit::Years, 4.99),
    (6, TimeUnit::Years, 5.11),
    (7, TimeUnit::Years, 5.23),
    (8, TimeUnit::Years, 5.33),
    (9, TimeUnit::Years, 5.41),
    (10, TimeUnit::Years, 5.47),
    (12, TimeUnit::Years, 5.60),
    (15, TimeUnit::Years, 5.75),
    (20, TimeUnit::Years, 5.89),
    (25, TimeUnit::Years, 5.95),
    (30, TimeUnit::Years, 5.96),
];

fn main() {
    // --- 1. Settings (D5: explicit, no global singleton) ---------------------
    // Evaluation date MUST be set before helpers are built; under D5 an unset
    // date is an Err, not a clock fallback.
    let calendar = Target::new();
    let today = calendar.adjust(
        Date::new(15, Month::June, 2026),
        BusinessDayConvention::Following,
    );
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);

    // Spot / settlement date = today + 2 business days; this is the curve's
    // reference date.
    let settlement = calendar.advance(
        today,
        2,
        TimeUnit::Days,
        BusinessDayConvention::Following,
        false,
    );

    // --- 2. Rate helpers -----------------------------------------------------
    // Each helper's index takes an EMPTY forwarding handle: the curve links
    // itself into the helpers during the bootstrap (passing the curve handle
    // here would create the circular dependency).
    let mut instruments: Vec<Shared<dyn RateHelper>> = Vec::new();

    // Deposit helpers. Euribor::new rejects TimeUnit::Days tenors, so the
    // shortest deposit here is 1W.
    for (n, units, rate) in DEPOSIT_DATA {
        let quote = Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>);
        let index = Euribor::new(Period::new(n, units), Handle::empty(), settings.clone())
            .expect("deposit tenor is valid");
        instruments.push(DepositRateHelper::new(quote, &index) as Shared<dyn RateHelper>);
    }

    // Swap helpers: annual 30/360 fixed leg vs 6M Euribor float leg.
    for (n, units, rate) in SWAP_DATA {
        let quote = Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>);
        let euribor6m = Euribor::six_months(Handle::empty(), settings.clone());
        instruments.push(SwapRateHelper::new(
            quote,
            Period::new(n, units),
            calendar.clone(),
            Frequency::Annual,
            BusinessDayConvention::Unadjusted,
            Thirty360::with_convention(Convention::BondBasis),
            &euribor6m,
        ) as Shared<dyn RateHelper>);
    }

    // --- 3. Build the curve --------------------------------------------------
    // <Discount, LogLinear>: node values are discount factors, log-linearly
    // interpolated. Construction is cheap; the bootstrap runs lazily on the
    // first read (discount / max_date / dates).
    let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        settlement,
        instruments,
        Actual360::new(),
        LogLinear,
    )
    .expect("curve constructed");

    // Forcing the bootstrap (and surfacing any bootstrap error) up front.
    let nodes = curve.dates().expect("bootstrap succeeds");
    println!("bootstrapped {} curve nodes", nodes.len());

    // --- 4. Query discount factors and zero rates ----------------------------
    // Pick a couple of forward dates inside the curve's range (extrapolate =
    // false): the 5Y and 10Y swap pillars.
    for years in [5, 10] {
        let date = settlement + Period::new(years, TimeUnit::Years);

        // discount_date(date): DF from the reference date to `date`.
        let df = curve
            .discount_date(date, false)
            .expect("date within curve range");

        // discount(t): same, but by time measured in the curve's own day count.
        let t = curve.time_from_reference(date).expect("time");
        let df_t = curve.discount(t, false).expect("t within range");

        // zero_rate(t): continuously-compounded zero rate for time t.
        let zero: Rate = curve
            .zero_rate(t, Compounding::Continuous, Frequency::Annual, false)
            .expect("zero rate")
            .rate();

        println!(
            "{years:>2}Y  t={t:.4}  DF(date)={df:.8}  DF(t)={df_t:.8}  zero(cont)={:.6}%",
            zero * 100.0
        );
    }
}
