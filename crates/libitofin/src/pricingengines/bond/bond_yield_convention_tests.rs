//! The QuantLib 1.44 `bonds.cpp` oracles for the stepwise yield analytics:
//! `testBasisPointValue` (:1759), `testYieldInFinalCouponPeriod` (:1952)
//! and `testYieldInFinalCouponPeriodWithLateRedemption` (:1980).

use super::*;
use crate::cashflows::FixedRateLeg;
use crate::instruments::BondPrice;
use crate::instruments::FixedRateBond;
use crate::interestrate::{Compounding, InterestRate};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::calendars::unitedstates::{self, UnitedStates};
use crate::time::date::Month;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounters::actualactual::{ActualActual, Convention};
use crate::time::daycounters::thirty360::{Convention as Thirty360Convention, Thirty360};
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::Natural;

const UNADJUSTED: BusinessDayConvention = BusinessDayConvention::Unadjusted;
const STC: Compounding = Compounding::SimpleThenCompounded;
const CTS: Compounding = Compounding::CompoundedThenSimple;

fn settings_at(today: Date) -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    settings
}

#[allow(clippy::too_many_arguments)]
fn fixed_rate_bond(
    settlement_days: Natural,
    face: Real,
    schedule: Schedule,
    coupon: Rate,
    day_counter: DayCounter,
    issue: Option<Date>,
    settings: Shared<Settings<Date>>,
) -> Bond {
    FixedRateBond::new(
        settlement_days,
        face,
        schedule,
        vec![coupon],
        day_counter,
        UNADJUSTED,
        100.0,
        issue,
        None,
        None,
        NullCalendar::new(),
        UNADJUSTED,
        false,
        None,
        settings,
    )
    .unwrap()
    .into_bond()
}

fn assert_close(what: &str, value: Real, expected: Real, tolerance: Real) {
    assert!(
        (value - expected).abs() <= tolerance,
        "{what}: {value} vs {expected}"
    );
}

/// `checkSensitivities` (`bonds.cpp:1900-1931`): the modified duration and
/// the convexity against central finite differences of the dirty price.
fn check_sensitivities(
    bond: &Bond,
    day_counter: &DayCounter,
    settlement: Date,
    rate: Rate,
    compounding: Compounding,
) {
    let shift = 1.0e-4;
    let yield_at =
        |y: Rate| InterestRate::new(y, day_counter.clone(), compounding, Frequency::Semiannual);
    let dirty = |y: Rate| {
        BondFunctions::dirty_price_at_yield(bond, &yield_at(y).unwrap(), Some(settlement)).unwrap()
    };
    let (price, up, down) = (dirty(rate), dirty(rate + shift), dirty(rate - shift));
    let expected_duration = -(up - down) / (2.0 * shift * price);
    let expected_convexity = (up - 2.0 * price + down) / (shift * shift * price);

    let yield_rate = yield_at(rate).unwrap();
    let duration =
        BondFunctions::duration(bond, &yield_rate, Duration::Modified, Some(settlement)).unwrap();
    assert_close(
        &format!("{compounding} modified duration"),
        duration,
        expected_duration,
        1e-7,
    );
    let convexity = BondFunctions::convexity(bond, &yield_rate, Some(settlement)).unwrap();
    assert_close(
        &format!("{compounding} convexity"),
        convexity,
        expected_convexity,
        1e-5,
    );
}

fn solve(
    bond: &Bond,
    price: BondPrice,
    day_counter: &DayCounter,
    compounding: Compounding,
    settlement: Date,
    guess: Option<Rate>,
) -> Rate {
    BondFunctions::yield_rate(
        bond,
        price,
        day_counter.clone(),
        compounding,
        Frequency::Semiannual,
        Some(settlement),
        Some(1e-10),
        Some(100),
        guess,
    )
    .unwrap()
}

/// `testBasisPointValue` (:1759): the 1.44 values of the table at
/// `:1798-1806`, and each one against a one-basis-point reprice (`:1817-1820`).
#[test]
fn the_basis_point_value_matches_quantlib_and_a_one_basis_point_reprice() {
    let face = 1_000_000.0;
    let settings = settings_at(Date::new(29, Month::January, 2024));
    let schedule = Schedule::new(
        Date::new(15, Month::November, 2023),
        Date::new(15, Month::August, 2033),
        Period::new(6, TimeUnit::Months),
        UnitedStates::new(unitedstates::Market::GovernmentBond),
        UNADJUSTED,
        UNADJUSTED,
        DateGeneration::Forward,
        false,
        Date::null(),
        Date::null(),
    );
    let day_counter = Thirty360::with_convention(Thirty360Convention::USA);
    let bond = fixed_rate_bond(
        1,
        face,
        schedule,
        0.045,
        day_counter.clone(),
        None,
        settings,
    );
    let default_settlement = bond.settlement_date(None).unwrap();
    let (comp, freq) = (Compounding::Compounded, Frequency::Semiannual);

    let irr = BondFunctions::yield_rate(
        &bond,
        BondPrice::Clean(102.890625),
        day_counter.clone(),
        comp,
        freq,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert_close("yield", irr, 0.041301, 1e-6);

    let rate = InterestRate::new(irr, day_counter.clone(), comp, freq).unwrap();
    let cases = [
        (None, -795.098043),
        (Some(default_settlement), -795.098043),
        (Some(Date::new(12, Month::February, 2024)), -792.789400),
    ];
    for (settlement, expected) in cases {
        let bpv = BondFunctions::basis_point_value(&bond, &rate, settlement).unwrap();
        assert_close("basis-point value", bpv, expected, 1e-6);

        let clean = |y: Rate| {
            let rate = InterestRate::new(y, day_counter.clone(), comp, freq).unwrap();
            BondFunctions::clean_price_at_yield(&bond, &rate, settlement).unwrap()
        };
        let reprice = (clean(irr + 0.0001) - clean(irr)) * face / 100.0;
        assert_close("basis-point value against a reprice", bpv, reprice, 1e-3);
    }
}

/// The ten-year 2.125% semiannual bond of `FinalCouponPeriodVars`
/// (`bonds.cpp:1937-1950`), settling on 5 March 2026 inside its final period.
fn final_coupon_vars() -> (Schedule, DayCounter, Date, Date, Date) {
    let (issue, maturity) = (
        Date::new(1, Month::June, 2016),
        Date::new(1, Month::June, 2026),
    );
    let schedule = Schedule::new(
        issue,
        maturity,
        Period::new(6, TimeUnit::Months),
        NullCalendar::new(),
        UNADJUSTED,
        UNADJUSTED,
        DateGeneration::Backward,
        false,
        Date::null(),
        Date::null(),
    );
    let day_counter = ActualActual::with_convention(Convention::ISMA);
    (
        schedule,
        day_counter,
        issue,
        maturity,
        Date::new(5, Month::March, 2026),
    )
}

const FINAL_COUPON_RATE: Rate = 0.02125;
const FINAL_COUPON_CLEAN: BondPrice = BondPrice::Clean(100.185);

/// `testYieldInFinalCouponPeriod` (:1952): with one period left both
/// composite conventions are simple and agree on 1.349867886%.
#[test]
fn both_composite_yields_agree_in_the_final_coupon_period() {
    let (schedule, day_counter, issue, _, settlement) = final_coupon_vars();
    let settings = settings_at(settlement);
    let bond = fixed_rate_bond(
        0,
        100.0,
        schedule,
        FINAL_COUPON_RATE,
        day_counter.clone(),
        Some(issue),
        settings,
    );
    for compounding in [STC, CTS] {
        let irr = solve(
            &bond,
            FINAL_COUPON_CLEAN,
            &day_counter,
            compounding,
            settlement,
            None,
        );
        assert_close(&format!("{compounding} yield"), irr, 0.013498678862, 1e-9);
    }
}

/// `testYieldInFinalCouponPeriodWithLateRedemption` (:1980): the redemption
/// is paid three days after the final coupon. The faceAmount `Bond`
/// constructor of `:2001` is not ported (#1424), so the bond is assembled
/// from the coupons and a single redemption, which gives it the same leg and
/// notional schedule.
#[test]
fn a_late_redemption_splits_the_composite_yields() {
    let (schedule, day_counter, issue, maturity, settlement) = final_coupon_vars();
    let settings = settings_at(settlement);
    let leg = FixedRateLeg::new(schedule)
        .with_notional(100.0)
        .with_coupon_rate(
            FINAL_COUPON_RATE,
            day_counter.clone(),
            Compounding::Simple,
            Frequency::Annual,
        )
        .unwrap()
        .with_payment_adjustment(UNADJUSTED)
        .build()
        .unwrap();
    let redemption_date = maturity + 3;
    let mut bond = Bond::new(0, NullCalendar::new(), Some(issue), leg, settings).unwrap();
    bond.set_single_redemption(100.0, 100.0, redemption_date)
        .unwrap();
    bond.set_maturity_date(redemption_date);

    let stc = solve(
        &bond,
        FINAL_COUPON_CLEAN,
        &day_counter,
        STC,
        settlement,
        None,
    );
    assert_close("STC yield", stc, 0.013059382713, 1e-9);
    let cts = solve(
        &bond,
        FINAL_COUPON_CLEAN,
        &day_counter,
        CTS,
        settlement,
        None,
    );
    assert_close("CTS yield", cts, 0.013079316928, 1e-9);

    check_sensitivities(&bond, &day_counter, settlement, stc, STC);
    check_sensitivities(&bond, &day_counter, settlement, cts, CTS);

    let after_final_coupon = Date::new(2, Month::June, 2026);
    let late_price = BondPrice::Clean(99.99);
    let late_stc = solve(
        &bond,
        late_price,
        &day_counter,
        STC,
        after_final_coupon,
        None,
    );
    assert_close("late STC yield", late_stc, 0.018251825183, 1e-9);
    let late_cts = solve(
        &bond,
        late_price,
        &day_counter,
        CTS,
        after_final_coupon,
        None,
    );
    assert_close("late CTS yield", late_cts, late_stc, 1e-12);
}
