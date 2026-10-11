//! Native QuantLib 1.43 zero-coupon call/put tree fixtures and lifecycle checks.

#[path = "fixtures/medium5_callable_zero/support.rs"]
mod support;

use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    BondArguments, BondPrice, Callability, CallabilityType, CallableBondArguments,
    CallableZeroCouponBond,
};
use libitofin::pricingengine::Arguments;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::Date;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use support::{attach, bond, calls, close, curve, date, flat, settings};

#[test]
fn twelve_native_nonflat_call_put_settlement_and_payment_cases() {
    for row in include_str!("fixtures/medium5_callable_zero/reference.tsv")
        .lines()
        .skip(1)
    {
        let columns: Vec<&str> = row.split('\t').collect();
        let settings = settings();
        let convention = if columns[1] == "Following" {
            Bdc::Following
        } else {
            Bdc::Preceding
        };
        let mut bond = bond(
            settings.clone(),
            calls(columns[2]),
            1234567.89,
            columns[3].parse().unwrap(),
            convention,
        );
        let curve = if columns[0] == "negative_rates" {
            flat(-0.012)
        } else {
            curve()
        };
        attach(&mut bond, Handle::new(curve), settings);
        close(bond.npv().unwrap(), columns[4].parse().unwrap(), 2e-8);
        close(
            bond.settlement_value().unwrap(),
            columns[5].parse().unwrap(),
            2e-8,
        );
        close(
            bond.dirty_price().unwrap(),
            columns[6].parse().unwrap(),
            2e-12,
        );
        close(
            bond.clean_price().unwrap(),
            columns[7].parse().unwrap(),
            2e-12,
        );
        assert_eq!(bond.bond().settlement_date(None).unwrap(), date(columns[8]));
        assert_eq!(bond.bond().redemptions()[0].date(), date(columns[9]));
        assert_eq!(bond.bond().maturity_date().unwrap(), date("2031-10-12"));
        assert_eq!(bond.bond().cashflows().len(), 1);
        assert_eq!(bond.bond().accrued_amount(None).unwrap(), 0.0);
        assert_eq!(bond.valuation_date().unwrap(), date("2026-10-12"));
        assert!(bond.base().is_calculated());
    }
}

#[test]
fn face_scaling_and_extreme_quote_normalization() {
    let settings = settings();
    let mut small = bond(
        settings.clone(),
        calls("2029-02-11,P,Dirty,95.25"),
        100.0,
        107.5,
        Bdc::Following,
    );
    let mut large = bond(
        settings.clone(),
        calls("2029-02-11,P,Dirty,95.25"),
        1e306,
        107.5,
        Bdc::Following,
    );
    attach(&mut small, Handle::new(curve()), settings.clone());
    attach(&mut large, Handle::new(curve()), settings);
    close(large.npv().unwrap() / 1e304, small.npv().unwrap(), 2e-12);
    close(
        large.clean_price().unwrap(),
        small.clean_price().unwrap(),
        2e-12,
    );
}

#[test]
fn argument_setup_filters_aligned_types_and_preserves_coincident_order() {
    let bond = bond(
        settings(),
        calls(
            "2026-10-13,P,Clean,120;2026-10-14,C,Dirty,50;2026-10-15,C,Clean,85;2026-10-15,P,Dirty,90",
        ),
        1000.0,
        107.5,
        Bdc::Following,
    );
    let mut args = CallableBondArguments::default();
    args.coupon_dates.push(Date::null());
    bond.setup_arguments(&mut args).unwrap();
    args.validate().unwrap();
    assert!(args.coupon_dates.is_empty());
    assert!(args.coupon_amounts.is_empty());
    assert_eq!(args.callability_prices, vec![85.0, 90.0]);
    assert_eq!(
        args.callability_types,
        vec![CallabilityType::Call, CallabilityType::Put]
    );
    assert_eq!(args.callability_dates, vec![date("2026-10-15"); 2]);
    assert_eq!(args.redemption, 1075.0);
    assert_eq!(bond.callability().len(), 4);
    assert!(
        bond.setup_arguments(&mut BondArguments {
            settlement_date: None,
            cashflows: vec![],
            calendar: Target::new()
        })
        .is_err()
    );
}

#[test]
fn quote_relink_reset_freeze_and_date_updates_use_one_cache() {
    use libitofin::interestrate::Compounding;
    use libitofin::time::frequency::Frequency;
    let settings = settings();
    let quote = shared(SimpleQuote::new(0.025));
    let initial: Shared<dyn YieldTermStructure> = shared(FlatForward::new(
        date("2026-10-12"),
        Handle::new(quote.clone() as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let handle = RelinkableHandle::new(initial);
    let rights = calls("2028-06-17,C,Clean,98.75");
    let mut bond = bond(
        settings.clone(),
        rights.clone(),
        1000.0,
        107.5,
        Bdc::Following,
    );
    attach(&mut bond, handle.handle().clone(), settings.clone());
    let initial = bond.npv().unwrap();
    assert_eq!(bond.npv().unwrap(), initial);
    assert!(std::ptr::eq(bond.base(), bond.bond().base()));
    bond.base_mut().freeze();
    quote.set_value(0.04);
    assert!(!bond.base().is_calculated());
    assert_eq!(bond.npv().unwrap(), initial);
    bond.base_mut().unfreeze();
    let changed = bond.npv().unwrap();
    assert!((changed - initial).abs() > 1.0);
    quote.set_value(f64::NAN);
    assert!(bond.npv().is_err());
    quote.set_value(0.04);
    close(bond.npv().unwrap(), changed, 2e-11);
    handle.reset();
    assert!(!bond.base().is_calculated());
    assert!(bond.settlement_value().is_err());
    handle.link_to(curve());
    let relinked = bond.clean_price().unwrap();
    let mut fresh = support::bond(settings.clone(), rights, 1000.0, 107.5, Bdc::Following);
    attach(&mut fresh, Handle::new(curve()), settings.clone());
    close(relinked, fresh.clean_price().unwrap(), 2e-12);
    settings.set_evaluation_date(date("2026-10-13"));
    assert!(!bond.base().is_calculated());
    close(
        bond.clean_price().unwrap(),
        fresh.clean_price().unwrap(),
        2e-12,
    );
    settings.set_evaluation_date(date("2031-10-14"));
    assert!(bond.is_expired().unwrap());
    assert_eq!(bond.npv().unwrap(), 0.0);
    assert_eq!(bond.settlement_value().unwrap(), 0.0);
    assert_eq!(bond.dirty_price().unwrap(), 0.0);
    assert_eq!(bond.clean_price().unwrap(), 0.0);
}

#[test]
fn constructors_and_missing_engine_fail_closed() {
    let settings = settings();
    let construct = |face, redemption, maturity, issue, rights, convention| {
        CallableZeroCouponBond::new(
            2,
            face,
            Target::new(),
            maturity,
            Actual365Fixed::new(),
            convention,
            redemption,
            issue,
            rights,
            settings.clone(),
        )
    };
    let maturity = date("2031-10-12");
    for (face, redemption) in [
        (0.0, 100.0),
        (-1.0, 100.0),
        (f64::NAN, 100.0),
        (f64::INFINITY, 100.0),
        (100.0, -1.0),
        (100.0, f64::NAN),
        (100.0, f64::INFINITY),
        (f64::MAX, 200.0),
    ] {
        assert!(construct(face, redemption, maturity, None, vec![], Bdc::Following).is_err());
    }
    for (maturity, issue, rights, convention) in [
        (Date::null(), None, vec![], Bdc::Following),
        (maturity, Some(Date::null()), vec![], Bdc::Following),
        (maturity, Some(date("2031-10-14")), vec![], Bdc::Following),
        (
            maturity,
            None,
            calls("2031-10-13,C,Clean,90"),
            Bdc::Following,
        ),
        (
            maturity,
            None,
            calls("2031-10-12,C,Clean,90"),
            Bdc::Preceding,
        ),
    ] {
        assert!(construct(100.0, 100.0, maturity, issue, rights, convention).is_err());
    }
    for price in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(
            Callability::new(
                BondPrice::Dirty(price),
                CallabilityType::Call,
                date("2028-06-17")
            )
            .is_err()
        );
    }
    let mut missing = bond(settings, vec![], 100.0, 100.0, Bdc::Following);
    assert!(missing.npv().is_err());
    assert!(missing.clean_price().is_err());
    assert!(!missing.base().is_calculated());
}

#[test]
fn silent_cashflow_flags_refresh_expiry_cache_and_preserve_freeze() {
    let settings = settings();
    settings.set_evaluation_date(date("2031-10-13"));
    let mut bond = CallableZeroCouponBond::new(
        0,
        1000.0,
        Target::new(),
        date("2031-10-12"),
        Actual365Fixed::new(),
        Bdc::Following,
        107.5,
        None,
        vec![],
        settings.clone(),
    )
    .unwrap();
    let curve = curve();
    let redemption_value = 1075.0 * curve.discount_date(date("2031-10-13"), false).unwrap();
    attach(&mut bond, Handle::new(curve), settings.clone());
    assert_eq!(bond.npv().unwrap(), 0.0);
    bond.base_mut().freeze();
    settings.set_include_reference_date_events(true);
    assert_eq!(bond.npv().unwrap(), 0.0);
    bond.base_mut().unfreeze();
    close(bond.npv().unwrap(), redemption_value, 2e-11);
    settings.set_include_reference_date_events(false);
    assert_eq!(bond.npv().unwrap(), 0.0);
    settings.set_include_todays_cash_flows(Some(true));
    close(bond.npv().unwrap(), redemption_value, 2e-11);
    close(bond.settlement_value().unwrap(), 1075.0, 2e-11);
    assert_eq!(bond.clean_price().unwrap(), 0.0);
    bond.base_mut().freeze();
    settings.set_include_reference_date_events(true);
    close(bond.npv().unwrap(), redemption_value, 2e-11);
    assert!(!bond.base().is_calculated());
    bond.base_mut().unfreeze();
    close(bond.npv().unwrap(), redemption_value, 2e-11);
    assert!(bond.base().is_calculated());
    settings.set_include_todays_cash_flows(Some(false));
    assert_eq!(bond.npv().unwrap(), 0.0);
    settings.set_include_todays_cash_flows(None);
    close(bond.npv().unwrap(), redemption_value, 2e-11);
    bond.base_mut().freeze();
    settings.set_include_reference_date_events(false);
    assert_eq!(bond.npv().unwrap(), 0.0);
    bond.base_mut().unfreeze();
}

#[test]
fn mixed_past_directions_price_as_the_correctly_pruned_native_schedule() {
    let settings = settings();
    let mut bond = bond(
        settings.clone(),
        calls("2026-10-13,P,Clean,120;2026-10-14,C,Dirty,50;2026-10-15,C,Clean,85"),
        1234567.89,
        107.5,
        Bdc::Following,
    );
    attach(&mut bond, Handle::new(curve()), settings);
    let native = include_str!("fixtures/medium5_callable_zero/reference.tsv")
        .lines()
        .find(|row| row.starts_with("settlement_filter\t"))
        .unwrap();
    let columns: Vec<&str> = native.split('\t').collect();
    close(bond.npv().unwrap(), columns[4].parse().unwrap(), 2e-8);
    close(
        bond.clean_price().unwrap(),
        columns[7].parse().unwrap(),
        2e-12,
    );
}
