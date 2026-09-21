use super::*;
use libitofin::indexes::ibor::Sofr;
use libitofin::interestrate::Compounding;
use libitofin::shared::shared;
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use std::ptr::null_mut;

#[test]
fn malformed_future_inputs_leave_session_usable() {
    let mut context = Context::default();
    let settings = shared(Settings::new());
    let today = Date::new(15, Month::March, 2024);
    settings.set_evaluation_date(today);
    let curve = shared(FlatForward::with_rate(
        today,
        0.03,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let index = context
        .insert(shared(Sofr::new(
            Handle::new(curve as Shared<dyn YieldTermStructure>),
            settings.clone(),
        )))
        .unwrap();
    let settings_id = context.insert(settings).unwrap();
    let price = context
        .insert(shared(libitofin::quotes::SimpleQuote::new(99.0)))
        .unwrap();
    let config = || ItofinOvernightFutureConfig {
        index,
        value_date: Date::new(20, Month::March, 2024).serial_number(),
        maturity_date: Date::new(20, Month::June, 2024).serial_number(),
        convexity: 0,
        averaging: 1,
    };
    let mut id = 0;
    unsafe {
        assert_ne!(
            itofin_overnight_future_new(&mut context, config(), null_mut(), null_mut()),
            0
        );
        for kind in [-1, 2, i32::MAX] {
            let mut a = config();
            a.averaging = kind;
            assert_ne!(
                itofin_overnight_future_new(&mut context, a, &mut id, null_mut()),
                0
            );
        }
        let mut a = config();
        a.maturity_date = a.value_date;
        assert_ne!(
            itofin_overnight_future_new(&mut context, a, &mut id, null_mut()),
            0
        );
        for custom in [
            0,
            today.serial_number(),
            Date::new(20, Month::July, 2024).serial_number(),
        ] {
            assert_ne!(
                itofin_overnight_future_helper_new(
                    &mut context,
                    config(),
                    price,
                    2,
                    custom,
                    &mut id,
                    null_mut()
                ),
                0
            );
        }
        assert_ne!(
            itofin_sofr_future_helper_new(
                &mut context,
                ItofinSofrFutureHelperConfig {
                    price,
                    month: 13,
                    year: 2024,
                    frequency: 4,
                    settings: settings_id,
                    convexity: 0,
                    pillar: 1,
                    custom_date: 0
                },
                &mut id,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_overnight_future_new(&mut context, config(), &mut id, null_mut()),
            0
        );
        let mut result = 0.0;
        assert_ne!(
            itofin_overnight_future_value(&mut context, id, 99, &mut result, null_mut()),
            0
        );
        assert_eq!(
            itofin_overnight_future_value(&mut context, id, 0, &mut result, null_mut()),
            0
        );
        assert!(result.is_finite() && result > 96.0 && result < 98.0);
        assert_ne!(
            itofin_overnight_add_fixing(
                &mut context,
                index,
                today.serial_number(),
                f64::NAN,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_overnight_add_fixing(
                &mut context,
                index,
                today.serial_number(),
                0.03,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_overnight_future_helper_new(
                &mut context,
                config(),
                price,
                2,
                Date::new(20, Month::April, 2024).serial_number(),
                &mut id,
                null_mut()
            ),
            0
        );
    }
}
