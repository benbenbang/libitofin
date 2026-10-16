use crate::bond_api::NativeBond;
use crate::bond_forward_api::*;
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::ZeroCouponBond;
use libitofin::interestrate::Compounding;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::{yields::FlatForward, yieldtermstructure::YieldTermStructure};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention as Bdc,
    calendars::NullCalendar,
    date::{Date, Month},
    daycounters::actual365fixed::Actual365Fixed,
    frequency::Frequency,
};
use std::ptr::null_mut;

fn fixture() -> (Context, ItofinBondForwardConfig, Shared<Settings<Date>>) {
    let mut c = Context::new();
    let today = Date::new(2, Month::January, 2025);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let curve = Handle::new(shared(FlatForward::with_rate(
        today,
        0.03,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    let mut bond = ZeroCouponBond::new(
        2,
        NullCalendar::new(),
        100.,
        Date::new(2, Month::July, 2026),
        Bdc::Unadjusted,
        100.,
        None,
        settings.clone(),
    )
    .unwrap()
    .into_bond();
    bond.base_mut()
        .set_pricing_engine(shared_mut(DiscountingBondEngine::new(
            curve.clone(),
            None,
            settings.clone(),
        ))
            as libitofin::shared::SharedMut<dyn PricingEngine>);
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let bond = c
        .insert(NativeBond::new(bond, calendar.clone(), None).unwrap())
        .unwrap();
    let calendar = c.insert(calendar).unwrap();
    let day_counter = c.insert(Actual365Fixed::new()).unwrap();
    let discount = c.insert(curve).unwrap();
    (
        c,
        ItofinBondForwardConfig {
            bond,
            value_date: today.serial_number(),
            delivery_date: Date::new(2, Month::July, 2025).serial_number(),
            position: 0,
            strike: 105.,
            settlement_days: 0,
            day_counter,
            calendar,
            convention: 2,
            discount,
            income_discount: 0,
        },
        settings,
    )
}
#[test]
fn boundary_outputs_are_untouched_and_native_owner_survives_release() {
    let (mut c, a, _) = fixture();
    let mut id = 0;
    unsafe {
        assert_ne!(
            itofin_bond_forward_new(&mut c, a, null_mut(), null_mut()),
            0
        );
    }
    let (mut c, a, _) = fixture();
    let underlying = a.bond;
    unsafe {
        assert_eq!(itofin_bond_forward_new(&mut c, a, &mut id, null_mut()), 0);
        let mut value = 12345.;
        assert_ne!(
            itofin_bond_forward_value(&mut c, id, 9, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 12345.);
        assert_ne!(
            itofin_bond_forward_value(&mut c, id, 0, null_mut(), null_mut()),
            0
        );
        assert_eq!(itofin_handle_release(&mut c, underlying, null_mut()), 0);
        assert_eq!(
            itofin_bond_forward_value(&mut c, id, 0, &mut value, null_mut()),
            0
        );
        assert!(value.is_finite());
        assert_ne!(itofin_bond_forward_control(&mut c, id, 99, null_mut()), 0);
        let mut date = 12345;
        assert_ne!(
            itofin_bond_forward_integer(&mut c, id, 99, &mut date, null_mut()),
            0
        );
        assert_eq!(date, 12345);
        assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
        value = 12345.;
        assert_ne!(
            itofin_bond_forward_value(&mut c, id, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 12345.);
    }
}
#[test]
fn invalid_constructor_leaves_output_unchanged() {
    for bad in [0, 1, 2, 3, 4, 5] {
        let (mut c, mut a, _) = fixture();
        match bad {
            0 => a.position = 9,
            1 => a.strike = f64::NAN,
            2 => a.delivery_date = 0,
            3 => a.discount = a.bond,
            4 => a.convention = 99,
            _ => a.settlement_days = u32::MAX,
        };
        let mut id = 12345;
        unsafe {
            assert_ne!(itofin_bond_forward_new(&mut c, a, &mut id, null_mut()), 0);
        }
        assert_eq!(id, 12345);
    }
}

#[test]
fn strike_and_dates_do_not_require_underlying_pricing_or_evaluation() {
    let (mut c, a, settings) = fixture();
    let native = c.get::<NativeBond>(a.bond).unwrap();
    native
        .bond
        .with_instrument_mut(|bond| bond.base_mut().detach_pricing_engine());
    settings.reset_evaluation_date();
    let mut id = 0;
    unsafe {
        assert_eq!(itofin_bond_forward_new(&mut c, a, &mut id, null_mut()), 0);
        let mut value = -123.;
        assert_eq!(
            itofin_bond_forward_value(&mut c, id, 3, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 105.);
        let mut date = 0;
        assert_eq!(
            itofin_bond_forward_integer(&mut c, id, 1, &mut date, null_mut()),
            0
        );
        assert!(date > 0);
        assert_ne!(
            itofin_bond_forward_value(&mut c, id, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 105.);
    }
}
