//! Independent QuantLib 1.43 event-ordering and ex-coupon fixtures from
//! `tests/fixtures/high28/callable-edge-cases.json`.

use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    BondPrice, Callability, CallabilityType, CallableBondArguments, CallableFixedRateBond,
};
use libitofin::interestrate::Compounding;
use libitofin::models::shortrate::HullWhite;
use libitofin::pricingengine::Arguments;
use libitofin::pricingengines::bond::TreeCallableFixedRateBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::MakeSchedule;
use libitofin::time::timeunit::TimeUnit;

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}, tolerance {tolerance}"
    );
}

#[test]
fn five_independent_quantlib_coupon_and_exercise_ordering_cases() {
    let issue = Date::new(7, Month::July, 2026);
    let late = Date::new(3, Month::July, 2028);
    let maturity = Date::new(7, Month::July, 2031);
    let close_coupon = Date::new(5, Month::July, 2028);
    let off_coupon = Date::new(7, Month::August, 2028);
    let cases = [
        (
            issue,
            7,
            vec![(close_coupon, CallabilityType::Call, BondPrice::Clean(100.0))],
            1037.5336514697024,
            1037.7042189655183,
            103.77042189655182,
            103.74302463627785,
            0.02739726027396472,
        ),
        (
            late,
            7,
            vec![(off_coupon, CallabilityType::Call, BondPrice::Clean(100.0))],
            1001.3617981645008,
            1001.5264191134953,
            100.15264191134953,
            100.18003917162349,
            -0.02739726027396472,
        ),
        (
            late,
            7,
            vec![(off_coupon, CallabilityType::Call, BondPrice::Dirty(100.0))],
            997.127421443303,
            997.291346273157,
            99.72913462731572,
            99.75653188758969,
            -0.02739726027396472,
        ),
        (
            issue,
            0,
            vec![
                (
                    Date::new(7, Month::July, 2028),
                    CallabilityType::Call,
                    BondPrice::Clean(100.0),
                ),
                (
                    Date::new(7, Month::July, 2029),
                    CallabilityType::Put,
                    BondPrice::Clean(101.0),
                ),
            ],
            1038.1166736124476,
            1038.2873369553974,
            103.82873369553975,
            103.8013364352658,
            0.02739726027396472,
        ),
        (
            issue,
            0,
            vec![(maturity, CallabilityType::Call, BondPrice::Clean(100.0))],
            1091.1696468618893,
            1091.3490319583746,
            109.13490319583745,
            109.1075059355635,
            0.02739726027396472,
        ),
    ];
    for (reference, ex_days, events, npv, settlement, dirty, clean, accrued) in cases {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference);
        let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::with_rate(
            reference,
            0.03,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        let model = HullWhite::new(Handle::new(curve), 0.1, 0.01).unwrap();
        let schedule = MakeSchedule::new()
            .from(issue)
            .to(maturity)
            .with_frequency(Frequency::Semiannual)
            .with_calendar(NullCalendar::new())
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        let calls = events
            .into_iter()
            .map(|(date, kind, quote)| Callability::new(quote, kind, date).unwrap())
            .collect();
        let mut bond = CallableFixedRateBond::with_ex_coupon(
            2,
            1000.0,
            schedule,
            vec![0.05],
            Actual365Fixed::new(),
            BusinessDayConvention::Unadjusted,
            100.0,
            Some(issue),
            calls,
            (ex_days > 0).then_some(Period::new(ex_days, TimeUnit::Days)),
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            settings.clone(),
        )
        .unwrap();
        let mut args = CallableBondArguments::default();
        bond.setup_arguments(&mut args).unwrap();
        args.validate().unwrap();
        if reference == issue && ex_days == 7 {
            assert!(args.callability_prices[0] > 102.0);
            let next_coupon = bond
                .bond()
                .cashflows()
                .iter()
                .find(|flow| flow.date() == Date::new(7, Month::July, 2028))
                .unwrap();
            let coupon = next_coupon.as_coupon().unwrap();
            assert!(coupon.trades_ex_coupon_on(close_coupon));
            assert!(coupon.accrued_amount(close_coupon).unwrap() < 0.0);
            let indenture_accrued =
                coupon.accrued_amount(close_coupon).unwrap() + next_coupon.amount().unwrap();
            close(
                args.callability_prices[0],
                100.0 + indenture_accrued / 1000.0 * 100.0,
                1e-13,
            );
        }
        if reference == late {
            assert!(!args.coupon_dates.contains(&Date::new(7, Month::July, 2028)));
        }
        bond.base_mut().set_pricing_engine(shared_mut(
            TreeCallableFixedRateBondEngine::new(model, 100, settings).unwrap(),
        ));
        close(bond.npv().unwrap(), npv, 1e-10);
        close(bond.settlement_value().unwrap(), settlement, 1e-10);
        close(bond.dirty_price().unwrap(), dirty, 1e-11);
        close(bond.clean_price().unwrap(), clean, 1e-11);
        close(bond.bond().accrued_amount(None).unwrap(), accrued, 1e-14);
    }
}
