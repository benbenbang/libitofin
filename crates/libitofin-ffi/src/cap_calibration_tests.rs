use crate::boundary::*;
use crate::cap_calibration_api::*;
use libitofin::handle::Handle;
use libitofin::indexes::Euribor;
use libitofin::interestrate::Compounding;
use libitofin::models::HullWhite;
use libitofin::quotes::SimpleQuote;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::{yields::FlatForward, yieldtermstructure::YieldTermStructure};
use libitofin::time::{
    date::{Date, Month},
    daycounters::actual365fixed::Actual365Fixed,
    frequency::Frequency,
};

#[test]
fn cap_boundary_invalid_flags_buffers_and_recovery() {
    let mut context = Context::new();
    let settings = shared(Settings::new());
    let date = Date::new(15, Month::January, 2026);
    settings.set_evaluation_date(date);
    let curve = Handle::new(shared(FlatForward::with_rate(
        date,
        0.03,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    let index = shared(Euribor::six_months(curve.clone(), settings));
    let model = context
        .insert(HullWhite::new(curve.clone(), 0.05, 0.01).unwrap())
        .unwrap();
    let curve = context.insert(curve).unwrap();
    let index = context
        .insert(crate::indexes_api::NativeIbor::builtin(index))
        .unwrap();
    let quote = context.insert(shared(SimpleQuote::new(0.01))).unwrap();
    let dc = context.insert(Actual365Fixed::new()).unwrap();
    let config = || ItofinCapHelperConfig {
        length: 5,
        length_unit: 3,
        volatility: quote,
        index,
        fixed_frequency: 0,
        fixed_day_counter: dc,
        include_first_swaplet: 0,
        curve,
        error_type: 1,
        volatility_type: 1,
        shift: 0.0,
    };
    unsafe {
        let mut error = std::mem::zeroed::<ItofinError>();
        let mut helper = 0;
        let mut invalid = config();
        invalid.include_first_swaplet = 2;
        assert_ne!(
            itofin_cap_helper_new(&mut context, invalid, &mut helper, &mut error),
            0
        );
        let mut invalid = config();
        invalid.volatility_type = 8;
        assert_ne!(
            itofin_cap_helper_new(&mut context, invalid, &mut helper, &mut error),
            0
        );
        let mut invalid = config();
        invalid.length = i32::MAX;
        let code = itofin_cap_helper_new(&mut context, invalid, &mut helper, &mut error);
        assert_ne!(code, 0);
        assert_ne!(code, PANIC);
        assert_eq!(
            itofin_cap_helper_new(&mut context, config(), &mut helper, &mut error),
            0
        );
        let mut engine = 0;
        for times in [
            &[f64::NAN, 1.0][..],
            &[0.0, f64::INFINITY][..],
            &[-1.0, 1.0][..],
        ] {
            assert_ne!(
                itofin_tree_capfloor_engine_new(
                    &mut context,
                    model,
                    0,
                    times.as_ptr(),
                    times.len(),
                    &mut engine,
                    &mut error
                ),
                0
            );
        }
        assert_eq!(
            itofin_tree_capfloor_engine_new(
                &mut context,
                model,
                30,
                std::ptr::null(),
                0,
                &mut engine,
                &mut error
            ),
            0
        );
        assert_eq!(
            itofin_cap_helper_set_tree_engine(&mut context, helper, engine, &mut error),
            0
        );
        let mut required = 0;
        assert_eq!(
            itofin_cap_helper_times(
                &mut context,
                helper,
                std::ptr::null_mut(),
                0,
                &mut required,
                &mut error
            ),
            0
        );
        assert!(required > 1);
        let mut values = vec![0.0; required];
        assert_ne!(
            itofin_cap_helper_times(
                &mut context,
                helper,
                values.as_mut_ptr(),
                required - 1,
                &mut required,
                &mut error
            ),
            0
        );
        assert_eq!(
            itofin_cap_helper_times(
                &mut context,
                helper,
                values.as_mut_ptr(),
                values.len(),
                &mut required,
                &mut error
            ),
            0
        );
        let mut value = 0.0;
        assert_ne!(
            itofin_cap_helper_value(&mut context, helper, 1, f64::NAN, &mut value, &mut error),
            0
        );
        assert_eq!(
            itofin_cap_helper_value(&mut context, helper, 2, 0.0, &mut value, &mut error),
            0
        );
        assert!(value.is_finite() && value > 0.0);
    }
}
