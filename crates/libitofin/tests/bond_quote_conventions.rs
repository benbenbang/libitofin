//! QuantLib 1.43 compiled-oracle cases for `cashflows.cpp` spread overloads and
//! `bondfunctions.cpp` curve/yield prices. Reproduce with the High28 oracle
//! generator in `tests/fixtures/high28`. Price tolerance is 1e-10; inversion
//! tolerance is 1e-10. Values are independent of the Rust implementation.

use libitofin::cashflow::CashFlow;
use libitofin::cashflows::FixedRateCoupon;
use libitofin::instruments::{Bond, BondPrice};
use libitofin::interestrate::{Compounding, InterestRate};
use libitofin::pricingengines::bond::BondFunctions;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;

fn date(day: i32, month: Month, year: i32) -> Date {
    Date::new(day, month, year)
}

fn curve(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        date(1, Month::July, 2026),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

fn settings() -> Shared<Settings<Date>> {
    let s = shared(Settings::new());
    s.set_evaluation_date(date(1, Month::July, 2026));
    s
}

fn bond(next_notional: f64, ex_coupon: bool) -> Bond {
    let dates = [
        date(7, Month::July, 2026),
        date(7, Month::July, 2027),
        date(7, Month::July, 2028),
    ];
    let leg = [1000.0, next_notional]
        .into_iter()
        .enumerate()
        .map(|(i, notional)| {
            shared(FixedRateCoupon::from_rate(
                dates[i + 1],
                notional,
                0.05,
                Actual360::new(),
                dates[i],
                dates[i + 1],
                None,
                None,
                ex_coupon.then_some(date(1, Month::July, 2027 + i as i32)),
            )) as Shared<dyn CashFlow>
        })
        .collect();
    Bond::from_coupons(0, NullCalendar::new(), Some(dates[0]), leg, settings()).unwrap()
}

#[test]
fn bond_prices_and_yield_inversion_match_quantlib_at_ex_coupon_and_amortization() {
    let cases = [
        (
            1000.0,
            false,
            3,
            1000.0,
            5.013888888888896,
            106.96046041779313,
            101.94657152890423,
            105.45229276244838,
            100.43840387355948,
        ),
        (
            1000.0,
            false,
            7,
            1000.0,
            0.0,
            101.92667540306272,
            101.92667540306272,
            100.43500293835395,
            100.43500293835395,
        ),
        (
            1000.0,
            false,
            10,
            1000.0,
            0.04166666666667318,
            101.95216025738755,
            101.91049359072088,
            100.47225551159316,
            100.43058884492649,
        ),
        (
            1000.0,
            true,
            3,
            1000.0,
            -0.05555555555556424,
            101.892705506559,
            101.94826106211457,
            100.38535432563626,
            100.44090988119183,
        ),
        (
            1000.0,
            true,
            7,
            1000.0,
            0.0,
            101.92667540306272,
            101.92667540306272,
            100.43500293835395,
            100.43500293835395,
        ),
        (
            1000.0,
            true,
            10,
            1000.0,
            0.04166666666667318,
            101.95216025738755,
            101.91049359072088,
            100.47225551159316,
            100.43058884492649,
        ),
        (
            600.0,
            false,
            3,
            1000.0,
            5.013888888888896,
            106.19004710381152,
            101.17615821492262,
            105.27837760210868,
            100.26448871321978,
        ),
        (
            600.0,
            false,
            7,
            600.0,
            0.0,
            101.92667540306273,
            101.92667540306273,
            100.43500293835395,
            100.43500293835395,
        ),
        (
            600.0,
            false,
            10,
            600.0,
            0.04166666666667318,
            101.95216025738753,
            101.91049359072086,
            100.47225551159315,
            100.43058884492648,
        ),
        (
            600.0,
            true,
            3,
            1000.0,
            -0.05555555555556424,
            101.12229219257739,
            101.17784774813296,
            100.21143916529657,
            100.26699472085213,
        ),
        (
            600.0,
            true,
            7,
            600.0,
            0.0,
            101.92667540306273,
            101.92667540306273,
            100.43500293835395,
            100.43500293835395,
        ),
        (
            600.0,
            true,
            10,
            600.0,
            0.04166666666667318,
            101.95216025738753,
            101.91049359072086,
            100.47225551159315,
            100.43058884492648,
        ),
    ];
    let yield_rate = InterestRate::new(
        0.045,
        Actual360::new(),
        Compounding::Compounded,
        Frequency::Semiannual,
    )
    .unwrap();
    let curve = curve(0.03);
    for (
        next_notional,
        ex_coupon,
        day,
        notional,
        accrued,
        curve_dirty,
        curve_clean,
        yield_dirty,
        yield_clean,
    ) in cases
    {
        let bond = bond(next_notional, ex_coupon);
        let settlement = Some(date(day, Month::July, 2027));
        assert_eq!(bond.notional(settlement).unwrap(), notional);
        assert!((bond.accrued_amount(settlement).unwrap() - accrued).abs() < 1e-10);
        for (value, expected) in [
            (
                BondFunctions::dirty_price(&bond, curve.as_ref(), settlement).unwrap(),
                curve_dirty,
            ),
            (
                BondFunctions::clean_price(&bond, curve.as_ref(), settlement).unwrap(),
                curve_clean,
            ),
            (
                BondFunctions::dirty_price_at_yield(&bond, &yield_rate, settlement).unwrap(),
                yield_dirty,
            ),
            (
                BondFunctions::clean_price_at_yield(&bond, &yield_rate, settlement).unwrap(),
                yield_clean,
            ),
            (
                BondFunctions::dirty_price_from_yield(
                    &bond,
                    0.045,
                    Actual360::new(),
                    Compounding::Compounded,
                    Frequency::Semiannual,
                    settlement,
                )
                .unwrap(),
                yield_dirty,
            ),
            (
                BondFunctions::clean_price_from_yield(
                    &bond,
                    0.045,
                    Actual360::new(),
                    Compounding::Compounded,
                    Frequency::Semiannual,
                    settlement,
                )
                .unwrap(),
                yield_clean,
            ),
        ] {
            assert!(
                (value - expected).abs() < 1e-10,
                "{next_notional}/{ex_coupon}/{day}: {value} vs {expected}"
            );
        }
        for price in [BondPrice::Clean(yield_clean), BondPrice::Dirty(yield_dirty)] {
            let recovered = BondFunctions::yield_rate(
                &bond,
                price,
                Actual360::new(),
                Compounding::Compounded,
                Frequency::Semiannual,
                settlement,
                Some(1e-12),
                Some(100),
                None,
            )
            .unwrap();
            assert!((recovered - 0.045).abs() < 1e-10);
        }
    }
}

#[test]
fn yield_prices_reject_non_finite_yields_and_redeemed_bonds() {
    let settlement = Some(date(7, Month::July, 2026));
    let bond = bond(1000.0, false);
    assert!(
        BondFunctions::dirty_price_from_yield(
            &bond,
            f64::NAN,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
            settlement
        )
        .is_err()
    );
    assert!(
        BondFunctions::dirty_price_at_yield(
            &bond,
            &InterestRate::new(
                0.05,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual
            )
            .unwrap(),
            Some(date(7, Month::July, 2028))
        )
        .is_err()
    );
}
