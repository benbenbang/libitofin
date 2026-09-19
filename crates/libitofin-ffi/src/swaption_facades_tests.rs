//! Native handle ownership, cached QuantLib Eonia pricing and invalid-input checks.
use crate::boundary::*;
use crate::indexes_api::{
    itofin_eonia_new, itofin_overnight_component, itofin_overnight_fixing_days,
};
use crate::rates_api::{ItofinMakeSwapConfig, itofin_make_ois};
use crate::rates_options::*;
use libitofin::exercise::{EuropeanExercise, Exercise};
use libitofin::handle::Handle;
use libitofin::indexes::{Index, InterestRateIndex, OvernightIndex};
use libitofin::interestrate::Compounding;
use libitofin::pricingengines::BlackSwaptionEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::{yields::FlatForward, yieldtermstructure::YieldTermStructure};
use libitofin::time::{
    date::{Date, Month},
    daycounters::{
        actual365fixed::Actual365Fixed,
        thirty360::{Convention, Thirty360},
    },
    frequency::Frequency,
};
use std::ptr::null_mut;

#[test]
fn eonia_ois_swaption_c_handles_retain_dependencies_and_reprice() {
    let mut c = Context::new();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(13, Month::March, 2002));
    let settlement = Date::new(15, Month::March, 2002);
    let curve = |rate| {
        Handle::new(shared(FlatForward::with_rate(
            settlement,
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let discount = curve(0.05);
    let forward_id = c.insert(curve(0.04)).unwrap();
    let settings_id = c.insert(settings.clone()).unwrap();
    let mut index = 0;
    unsafe {
        assert_eq!(
            itofin_eonia_new(&mut c, forward_id, settings_id, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_eonia_new(&mut c, forward_id, settings_id, &mut index, null_mut()),
            0
        );
    }
    let core_index = c.get::<Shared<OvernightIndex>>(index).unwrap();
    assert_eq!(core_index.fixing_days(), 0);
    assert_eq!(core_index.currency().code(), "EUR");
    assert_eq!(core_index.fixing_calendar().name(), "TARGET");
    assert_eq!(core_index.day_counter().name(), "Actual/360");
    drop(core_index);
    unsafe {
        let mut days = 99;
        assert_eq!(
            itofin_overnight_fixing_days(&mut c, index, &mut days, null_mut()),
            0
        );
        assert_eq!(days, 0);
        assert_eq!(
            itofin_overnight_fixing_days(&mut c, index, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        let mut component = 0;
        assert_eq!(
            itofin_overnight_component(&mut c, index, 99, &mut component, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_overnight_component(&mut c, index, 0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_overnight_component(&mut c, forward_id, 0, &mut component, null_mut()),
            INVALID_HANDLE
        );
    }

    let fixed_dc = c
        .insert(Thirty360::with_convention(Convention::BondBasis))
        .unwrap();
    let config = ItofinMakeSwapConfig {
        tenor_length: 10,
        tenor_unit: 3,
        index,
        settings: settings_id,
        flags: 1 | 2 | 16,
        fixed_rate: 0.06,
        forward_length: 0,
        forward_unit: 0,
        effective_date: Date::new(19, Month::March, 2007).serial_number(),
        nominal: 0.0,
        fixed_length: 0,
        fixed_unit: 0,
        fixed_day_counter: fixed_dc,
        payment_lag: 0,
        discount: 0,
        averaging: 0,
    };
    let exercise = c
        .insert(
            shared(EuropeanExercise::new(Date::new(15, Month::March, 2007)))
                as Shared<dyn Exercise>,
        )
        .unwrap();
    let mut swap = 0;
    let mut option = 0;
    let vol = shared(SimpleQuote::new(0.20));
    let engine = c
        .insert(shared_mut(BlackSwaptionEngine::with_flat_vol(
            discount,
            Handle::new(vol.clone() as Shared<dyn Quote>),
            Actual365Fixed::new(),
            0.0,
            libitofin::pricingengines::swaption::CashAnnuityModel::SwapRate,
            settings,
        )))
        .unwrap();
    unsafe {
        assert_eq!(itofin_make_ois(&mut c, config, &mut swap, null_mut()), 0);
        assert_eq!(
            itofin_swaption_new(
                &mut c,
                swap,
                exercise,
                0,
                0,
                settings_id,
                &mut option,
                null_mut()
            ),
            INVALID_HANDLE
        );
        assert_eq!(
            itofin_swaption_from_ois(
                &mut c,
                swap,
                exercise,
                7,
                0,
                settings_id,
                &mut option,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_swaption_from_ois(
                &mut c,
                swap,
                exercise,
                0,
                0,
                settings_id,
                &mut option,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_rate_option_set_engine(&mut c, option, engine, 0, null_mut()),
            0
        );
        let mut value = 0.0;
        assert_eq!(
            itofin_rate_option_value(&mut c, option, 0, 0, &mut value, null_mut()),
            0
        );
        assert!((value - 0.014101075767).abs() <= 1e-12, "{value:.16}");
        for id in [
            swap,
            exercise,
            engine,
            index,
            forward_id,
            settings_id,
            fixed_dc,
        ] {
            assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
        }
        let initial = value;
        vol.set_value(0.30);
        assert_eq!(
            itofin_rate_option_value(&mut c, option, 0, 0, &mut value, null_mut()),
            0
        );
        assert!(value > initial);
        assert_eq!(
            itofin_swaption_details(&mut c, option, 99, &mut value, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(itofin_handle_release(&mut c, option, null_mut()), 0);
        assert_eq!(
            itofin_rate_option_value(&mut c, option, 0, 0, &mut value, null_mut()),
            INVALID_HANDLE
        );
    }
}

#[test]
fn make_swaption_c_builder_validates_options_and_preserves_forecast() {
    use libitofin::currency::Currency;
    use libitofin::indexes::{Euribor, SwapIndex};
    use libitofin::time::{
        businessdayconvention::BusinessDayConvention, calendars::target::Target,
        daycounters::actual360::Actual360, period::Period, timeunit::TimeUnit,
    };
    let mut c = Context::new();
    let settings = shared(Settings::new());
    let today = Date::new(9, Month::October, 2015);
    settings.set_evaluation_date(today);
    let curve = Handle::new(shared(FlatForward::with_rate(
        today,
        0.05,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    let ibor = shared(Euribor::six_months(curve, settings.clone()));
    let index = c
        .insert(shared(SwapIndex::new(
            "EuriborSwapIsdaFixA".into(),
            Period::new(5, TimeUnit::Years),
            2,
            Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BusinessDayConvention::ModifiedFollowing,
            Thirty360::with_convention(Convention::BondBasis),
            ibor,
            settings,
        )))
        .unwrap();
    let config = |flags| ItofinMakeSwaptionConfig {
        index,
        tenor_length: 1,
        tenor_unit: 3,
        flags,
        strike: f64::NAN,
        nominal: 1.0,
        fixing_date: 0,
        exercise_date: 0,
        exercise_calendar: 0,
        option_convention: 0,
        settlement_type: 0,
        settlement_method: 0,
        indexed_coupons: 2,
    };
    let mut option = 0;
    unsafe {
        for flags in [1, 4, 8, 16, 32] {
            assert_eq!(
                itofin_make_swaption(&mut c, config(flags), &mut option, null_mut()),
                INVALID_ARGUMENT
            );
        }
        for field in 0..4 {
            let mut invalid = config(0);
            match field {
                0 => invalid.settlement_type = 99,
                1 => invalid.settlement_method = 99,
                2 => invalid.option_convention = 99,
                _ => invalid.tenor_unit = 99,
            }
            assert_eq!(
                itofin_make_swaption(&mut c, invalid, &mut option, null_mut()),
                INVALID_ARGUMENT
            );
        }
        assert_eq!(
            itofin_make_swaption(&mut c, config(0), null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_make_swaption(&mut c, config(0), &mut option, null_mut()),
            0
        );
        assert_eq!(itofin_handle_release(&mut c, index, null_mut()), 0);
        let mut value = 0.0;
        assert_eq!(
            itofin_swaption_details(&mut c, option, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(
            value,
            f64::from(Date::new(10, Month::October, 2016).serial_number())
        );
        assert_eq!(
            itofin_swaption_details(&mut c, option, 1, &mut value, null_mut()),
            0
        );
        assert!((value - 0.05202914613654939).abs() <= 1e-12, "{value:.16}");
    }
}
