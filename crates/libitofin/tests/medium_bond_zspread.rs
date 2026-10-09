//! Compiled QuantLib 1.43 bond z-spread oracles, price/spread tolerance 1e-10.

use libitofin::cashflow::CashFlow;
use libitofin::cashflows::FixedRateCoupon;
use libitofin::handle::Handle;
use libitofin::instruments::{Bond, BondPrice};
use libitofin::interestrate::Compounding;
use libitofin::pricingengines::bond::BondFunctions;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;

fn date(day: i32, year: i32) -> Date {
    Date::new(day, Month::July, year)
}

fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(date(1, 2026));
    settings
}

fn curve() -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        date(1, 2026),
        0.03,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}

fn bond(
    next_notional: f64,
    ex_coupon: bool,
    irregular: bool,
    settings: Shared<Settings<Date>>,
) -> Bond {
    let mut dates = vec![date(7, 2026), date(7, 2027)];
    if irregular {
        dates.push(Date::new(19, Month::October, 2027));
    }
    dates.push(date(7, 2028));
    let coupons = dates
        .windows(2)
        .enumerate()
        .map(|(i, dates)| {
            shared(FixedRateCoupon::from_rate(
                dates[1],
                if i == 0 { 1000.0 } else { next_notional },
                0.05,
                Actual360::new(),
                dates[0],
                dates[1],
                None,
                None,
                ex_coupon.then_some(dates[1] - 6),
            )) as Shared<dyn CashFlow>
        })
        .collect();
    Bond::from_coupons(0, NullCalendar::new(), Some(dates[0]), coupons, settings).unwrap()
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-10,
        "actual={actual:.17} expected={expected:.17}"
    );
}

#[test]
fn compiled_oracles_cover_ex_coupon_amortization_irregular_periods_and_settlement() {
    let fixtures = r#"0 1000.0 0 3 0 0.0125 105.6590912395323 100.6452023506434
0 1000.0 0 3 1 -0.005 107.47850508861964 102.46461619973074
0 1000.0 0 7 0 0.0125 100.63955314846812 100.63955314846812
0 1000.0 0 7 1 -0.005 102.43900159603652 102.43900159603652
0 1000.0 0 10 0 0.0125 100.67520263609899 100.63353596943232
0 1000.0 0 10 1 -0.005 102.46040365048923 102.41873698382255
0 1000.0 1 3 0 0.0125 100.5920401342704 100.64759568982598
0 1000.0 1 3 1 -0.005 102.41047247736286 102.46602803291842
0 1000.0 1 7 0 0.0125 100.63955314846812 100.63955314846812
0 1000.0 1 7 1 -0.005 102.43900159603652 102.43900159603652
0 1000.0 1 10 0 0.0125 100.67520263609899 100.63353596943232
0 1000.0 1 10 1 -0.005 102.46040365048923 102.41873698382255
0 400.0 0 3 0 0.0125 105.27554051439866 100.26165162550976
0 400.0 0 3 1 -0.005 106.0155116861177 101.0016227972288
0 400.0 0 7 0 0.0125 100.63955314846812 100.63955314846812
0 400.0 0 7 1 -0.005 102.43900159603652 102.43900159603652
0 400.0 0 10 0 0.0125 100.67520263609899 100.63353596943232
0 400.0 0 10 1 -0.005 102.46040365048924 102.41873698382257
0 400.0 1 3 0 0.0125 100.20848940913673 100.26404496469229
0 400.0 1 3 1 -0.005 100.94747907486092 101.0030346304165
0 400.0 1 7 0 0.0125 100.63955314846812 100.63955314846812
0 400.0 1 7 1 -0.005 102.43900159603652 102.43900159603652
0 400.0 1 10 0 0.0125 100.67520263609899 100.63353596943232
0 400.0 1 10 1 -0.005 102.46040365048924 102.41873698382257
1 1000.0 0 3 0 0.0125 105.70252747071606 100.68863858182716
1 1000.0 0 3 1 -0.005 107.5044232311283 102.4905343422394
1 1000.0 0 7 0 0.0125 100.68300989604927 100.68300989604927
1 1000.0 0 7 1 -0.005 102.46492695872244 102.46492695872244
1 1000.0 0 10 0 0.0125 100.71867477733737 100.6770081106707
1 1000.0 0 10 1 -0.005 102.48633442962803 102.44466776296136
1 1000.0 1 3 0 0.0125 100.63547636545417 100.69103192100974
1 1000.0 1 3 1 -0.005 102.43639061987153 102.4919461754271
1 1000.0 1 7 0 0.0125 100.68300989604927 100.68300989604927
1 1000.0 1 7 1 -0.005 102.46492695872244 102.46492695872244
1 1000.0 1 10 0 0.0125 100.71867477733737 100.6770081106707
1 1000.0 1 10 1 -0.005 102.48633442962803 102.44466776296136
1 400.0 0 3 0 0.0125 105.29291500687216 100.27902611798326
1 400.0 0 3 1 -0.005 106.02587894312116 101.01199005423226
1 400.0 0 7 0 0.0125 100.68300989604926 100.68300989604926
1 400.0 0 7 1 -0.005 102.46492695872243 102.46492695872243
1 400.0 0 10 0 0.0125 100.71867477733737 100.6770081106707
1 400.0 0 10 1 -0.005 102.48633442962802 102.44466776296134
1 400.0 1 3 0 0.0125 100.22586390161024 100.28141945716581
1 400.0 1 3 1 -0.005 100.95784633186439 101.01340188741995
1 400.0 1 7 0 0.0125 100.68300989604926 100.68300989604926
1 400.0 1 7 1 -0.005 102.46492695872243 102.46492695872243
1 400.0 1 10 0 0.0125 100.71867477733737 100.6770081106707
1 400.0 1 10 1 -0.005 102.48633442962802 102.44466776296134"#;
    for line in fixtures.lines() {
        let fields: Vec<f64> = line
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let bond = bond(fields[1], fields[2] != 0.0, fields[0] != 0.0, settings());
        let settlement = Some(date(fields[3] as i32, 2027));
        let comp = if fields[4] == 0.0 {
            Compounding::Continuous
        } else {
            Compounding::Compounded
        };
        let discount = curve();
        let dirty = BondFunctions::dirty_price_at_z_spread(
            &bond,
            discount.clone(),
            fields[5],
            comp,
            Frequency::Semiannual,
            settlement,
        )
        .unwrap();
        let clean = BondFunctions::clean_price_at_z_spread(
            &bond,
            discount.clone(),
            fields[5],
            comp,
            Frequency::Semiannual,
            settlement,
        )
        .unwrap();
        assert_close(dirty, fields[6]);
        assert_close(clean, fields[7]);
        assert_close(dirty - clean, bond.accrued_amount(settlement).unwrap());
        for price in [BondPrice::Clean(fields[7]), BondPrice::Dirty(fields[6])] {
            let root = BondFunctions::z_spread(
                &bond,
                price,
                discount.clone(),
                comp,
                Frequency::Semiannual,
                settlement,
                None,
                None,
                None,
            )
            .unwrap();
            assert_close(root, fields[5]);
            let root = BondFunctions::z_spread(
                &bond,
                price,
                discount.clone(),
                comp,
                Frequency::Semiannual,
                settlement,
                Some(1e-13),
                None,
                None,
            )
            .unwrap();
            let repriced = BondFunctions::dirty_price_at_z_spread(
                &bond,
                discount.clone(),
                root,
                comp,
                Frequency::Semiannual,
                settlement,
            )
            .unwrap();
            assert_close(repriced, fields[6]);
        }
    }
}

#[test]
fn defaults_honor_bond_settlement_and_cashflow_settings() {
    let settings = settings();
    settings.set_evaluation_date(date(7, 2027));
    settings.set_include_reference_date_events(true);
    settings.set_include_todays_cash_flows(Some(true));
    let bond = bond(400.0, true, true, settings.clone());
    let discount = curve();
    for comp in [
        Compounding::Simple,
        Compounding::Compounded,
        Compounding::Continuous,
        Compounding::SimpleThenCompounded,
        Compounding::CompoundedThenSimple,
    ] {
        let default = BondFunctions::dirty_price_at_z_spread(
            &bond,
            discount.clone(),
            0.0,
            comp,
            Frequency::Semiannual,
            None,
        )
        .unwrap();
        let explicit = BondFunctions::dirty_price_at_z_spread(
            &bond,
            discount.clone(),
            0.0,
            comp,
            Frequency::Semiannual,
            Some(date(7, 2027)),
        )
        .unwrap();
        assert_close(default, explicit);
        assert_close(
            default,
            BondFunctions::dirty_price(
                &bond,
                discount.current_link().unwrap().as_ref(),
                Some(date(7, 2027)),
            )
            .unwrap(),
        );
    }
    settings.set_evaluation_date(date(10, 2027));
    assert_close(
        BondFunctions::clean_price_at_z_spread(
            &bond,
            discount.clone(),
            0.0125,
            Compounding::Continuous,
            Frequency::Semiannual,
            None,
        )
        .unwrap(),
        BondFunctions::clean_price_at_z_spread(
            &bond,
            discount,
            0.0125,
            Compounding::Continuous,
            Frequency::Semiannual,
            Some(date(10, 2027)),
        )
        .unwrap(),
    );
}

#[test]
fn invalid_inputs_nontradable_bonds_and_solver_failures_are_errors() {
    let bond = bond(400.0, true, true, settings());
    let settlement = Some(date(3, 2027));
    for spread in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            BondFunctions::dirty_price_at_z_spread(
                &bond,
                curve(),
                spread,
                Compounding::Continuous,
                Frequency::Annual,
                settlement
            )
            .is_err()
        );
        assert!(
            BondFunctions::clean_price_at_z_spread(
                &bond,
                curve(),
                spread,
                Compounding::Continuous,
                Frequency::Annual,
                settlement
            )
            .is_err()
        );
    }
    for price in [BondPrice::Clean(f64::NAN), BondPrice::Dirty(f64::INFINITY)] {
        assert!(
            BondFunctions::z_spread(
                &bond,
                price,
                curve(),
                Compounding::Continuous,
                Frequency::Annual,
                settlement,
                None,
                None,
                None
            )
            .is_err()
        );
    }
    for accuracy in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            BondFunctions::z_spread(
                &bond,
                BondPrice::Clean(100.0),
                curve(),
                Compounding::Continuous,
                Frequency::Annual,
                settlement,
                Some(accuracy),
                None,
                None
            )
            .is_err()
        );
    }
    for guess in [f64::NAN, f64::INFINITY] {
        assert!(
            BondFunctions::z_spread(
                &bond,
                BondPrice::Clean(100.0),
                curve(),
                Compounding::Continuous,
                Frequency::Annual,
                settlement,
                None,
                None,
                Some(guess)
            )
            .is_err()
        );
    }
    for max in [0, 1] {
        assert!(
            BondFunctions::z_spread(
                &bond,
                BondPrice::Clean(100.0),
                curve(),
                Compounding::Continuous,
                Frequency::Annual,
                settlement,
                None,
                Some(max),
                None
            )
            .is_err()
        );
    }
    assert!(
        BondFunctions::dirty_price_at_z_spread(
            &bond,
            Handle::empty(),
            0.01,
            Compounding::Continuous,
            Frequency::Annual,
            settlement
        )
        .is_err()
    );
    for day in [7, 8] {
        let settlement = Some(date(day, 2028));
        assert!(
            BondFunctions::dirty_price_at_z_spread(
                &bond,
                curve(),
                0.01,
                Compounding::Continuous,
                Frequency::Annual,
                settlement
            )
            .is_err()
        );
        assert!(
            BondFunctions::clean_price_at_z_spread(
                &bond,
                curve(),
                0.01,
                Compounding::Continuous,
                Frequency::Annual,
                settlement
            )
            .is_err()
        );
        assert!(
            BondFunctions::z_spread(
                &bond,
                BondPrice::Clean(100.0),
                curve(),
                Compounding::Continuous,
                Frequency::Annual,
                settlement,
                None,
                None,
                None
            )
            .is_err()
        );
    }
    let empty = Bond::new(0, NullCalendar::new(), None, vec![], settings()).unwrap();
    assert!(
        BondFunctions::dirty_price_at_z_spread(
            &empty,
            curve(),
            0.01,
            Compounding::Continuous,
            Frequency::Annual,
            settlement
        )
        .is_err()
    );
}

#[test]
fn large_notional_normalizes_before_scaling_and_malformed_cashflows_error() {
    for (notional, rate) in [(1e308, 0.0), (1000.0, f64::NAN)] {
        let coupon: Shared<dyn CashFlow> = shared(FixedRateCoupon::from_rate(
            date(7, 2028),
            notional,
            rate,
            Actual360::new(),
            date(7, 2026),
            date(7, 2028),
            None,
            None,
            None,
        ));
        let bond = Bond::from_coupons(
            0,
            NullCalendar::new(),
            Some(date(7, 2026)),
            vec![coupon],
            settings(),
        )
        .unwrap();
        let settlement = Some(date(10, 2027));
        let dirty = BondFunctions::dirty_price_at_z_spread(
            &bond,
            curve(),
            0.0125,
            Compounding::Continuous,
            Frequency::Annual,
            settlement,
        );
        if rate.is_nan() {
            assert!(dirty.is_err());
            assert!(
                BondFunctions::z_spread(
                    &bond,
                    BondPrice::Clean(100.0),
                    curve(),
                    Compounding::Continuous,
                    Frequency::Annual,
                    settlement,
                    None,
                    None,
                    None
                )
                .is_err()
            );
        } else {
            let dirty = dirty.unwrap();
            assert_close(dirty, 100.0 * (-0.0425 * 363.0_f64 / 360.0).exp());
            let root = BondFunctions::z_spread(
                &bond,
                BondPrice::Dirty(dirty),
                curve(),
                Compounding::Continuous,
                Frequency::Annual,
                settlement,
                None,
                None,
                None,
            )
            .unwrap();
            assert_close(root, 0.0125);
            assert!(
                BondFunctions::z_spread(
                    &bond,
                    BondPrice::Dirty(400.0),
                    curve(),
                    Compounding::Continuous,
                    Frequency::Annual,
                    settlement,
                    None,
                    None,
                    None
                )
                .is_err()
            );
        }
    }
}
