use super::*;
use itofin_ffi::bates_calibration_api::itofin_bates_calibrate_with_options;
use itofin_ffi::constraint_api::{ItofinCalibrationOptions, itofin_boundary_constraint_new};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::optimization::{
    endcriteria::EndCriteria, levenbergmarquardt::LevenbergMarquardt, method::OptimizationMethod,
};
use libitofin::models::calibrationhelper::{CalibrationErrorType, CalibrationHelper};
use libitofin::models::equity::HestonModelHelper;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{
    calendars::nullcalendar::NullCalendar, frequency::Frequency, period::Period, timeunit::TimeUnit,
};
const QUOTES: [[f64; 4]; 20] = [
    [3.0, 80.0, 0.3340806031216575, 0.5707263449125093],
    [3.0, 90.0, 0.28770574448622066, 1.700183823528828],
    [3.0, 100.0, 0.2467900765362469, 4.620406614257564],
    [3.0, 110.0, 0.21872555918489292, 1.2859596016098496],
    [3.0, 120.0, 0.21640952251962955, 0.2410526708011516],
    [6.0, 80.0, 0.3091908164161894, 1.3970143828378212],
    [6.0, 90.0, 0.2824275545816448, 3.248192935495845],
    [6.0, 100.0, 0.25601626999733545, 6.631550400955093],
    [6.0, 110.0, 0.2329251509593595, 3.2564414117785083],
    [6.0, 120.0, 0.2188908438656418, 1.1037850458247094],
    [12.0, 80.0, 0.29526424386235267, 2.9657177832024506],
    [12.0, 90.0, 0.27898626981503616, 5.522080126292517],
    [12.0, 100.0, 0.26330493938674165, 9.316411873143176],
    [12.0, 110.0, 0.2486337305267027, 6.766898546585497],
    [12.0, 120.0, 0.2360517985897031, 3.6833348413947617],
    [24.0, 80.0, 0.28708418986294393, 5.3762822115089515],
    [24.0, 90.0, 0.27736643124409055, 8.529882049225694],
    [24.0, 100.0, 0.2684754291100181, 12.626668948799178],
    [24.0, 110.0, 0.26032981773834557, 12.099670592331066],
    [24.0, 120.0, 0.25291682030007095, 8.635896149778578],
];

#[test]
fn bates_independent_calibration_and_copied_options() {
    for jump_three in [false, true] {
        let today = Date::new(15, Month::January, 2026);
        let mut context = Context::new();
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let flat = |rate| {
            Handle::new(shared(FlatForward::with_rate(
                today,
                rate,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        let risk = flat(0.03);
        let dividend = flat(0.01);
        let spot = Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>);
        let start = if jump_three {
            [0.05, 1.5, 0.35, -0.6, 0.04, -0.06, 0.25, 0.3]
        } else {
            [0.05, 1.5, 0.35, -0.6, 0.04, -0.12, 0.18, 1.2]
        };
        let process = shared(
            libitofin::processes::BatesProcess::new(
                risk.clone(),
                dividend.clone(),
                spot,
                start[4],
                start[1],
                start[0],
                start[2],
                start[3],
                start[7],
                start[5],
                start[6],
            )
            .unwrap(),
        );
        let process = context.insert(process).unwrap();
        let mut model = 0;
        let mut constraint = 0;
        let mut helper_objects = Vec::new();
        let mut helper_ids = Vec::new();
        for row in QUOTES {
            let quote = Handle::new(shared(SimpleQuote::new(row[2])) as Shared<dyn Quote>);
            let helper = shared_mut(HestonModelHelper::new(
                Period::new(row[0] as i32, TimeUnit::Months),
                NullCalendar::new(),
                100.0,
                row[1],
                quote,
                risk.clone(),
                dividend.clone(),
                CalibrationErrorType::RelativePriceError,
                settings.clone(),
            ));
            helper_ids.push(context.insert(helper.clone()).unwrap());
            helper_objects.push(helper);
        }
        let tol = if jump_three { 1e-12 } else { 1e-8 };
        let method: SharedMut<dyn OptimizationMethod> =
            shared_mut(LevenbergMarquardt::new(tol, tol, tol, false));
        let method = context.insert(method).unwrap();
        let criteria = context
            .insert(
                EndCriteria::new(
                    if jump_three { 800 } else { 400 },
                    Some(if jump_three { 80 } else { 40 }),
                    tol,
                    tol,
                    Some(tol),
                )
                .unwrap(),
            )
            .unwrap();
        let fixed = if jump_three {
            [1_u8, 1, 1, 1, 1, 0, 0, 0]
        } else {
            [1_u8, 1, 1, 1, 1, 1, 1, 0]
        };
        let weights = [1.0; 20];
        let mut params = [0.0; 8];
        unsafe {
            assert_eq!(
                itofin_bates_model_new(&mut context, process, &mut model, null_mut()),
                0
            );
            assert_eq!(
                itofin_boundary_constraint_new(
                    &mut context,
                    -1.0,
                    2.0,
                    &mut constraint,
                    null_mut()
                ),
                0
            );
            let options = ItofinCalibrationOptions {
                constraint,
                weights: weights.as_ptr(),
                weights_len: 20,
                fix_parameters: fixed.as_ptr(),
                fix_parameters_len: 8,
            };
            assert_eq!(
                itofin_bates_calibrate_with_options(
                    &mut context,
                    model,
                    helper_ids.as_ptr(),
                    20,
                    method,
                    criteria,
                    144,
                    &options,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_bates_model_params(&mut context, model, params.as_mut_ptr(), 8, null_mut()),
                0
            );
            let expected = [0.05, 1.5, 0.35, -0.6, 0.04, -0.12, 0.18, 0.7];
            for i in 0..8 {
                assert!((params[i] - expected[i]).abs() <= 1e-6);
                if fixed[i] == 1 {
                    assert_eq!(params[i], start[i]);
                }
            }
            for helper in &helper_objects {
                assert!(helper.borrow_mut().calibration_error().unwrap().abs() <= 1e-8);
            }
            for bad in [-1.0, f64::NAN, f64::INFINITY] {
                let mut bad_weights = weights;
                bad_weights[0] = bad;
                let options = ItofinCalibrationOptions {
                    constraint: 0,
                    weights: bad_weights.as_ptr(),
                    weights_len: 20,
                    fix_parameters: fixed.as_ptr(),
                    fix_parameters_len: 8,
                };
                assert_ne!(
                    itofin_bates_calibrate_with_options(
                        &mut context,
                        model,
                        helper_ids.as_ptr(),
                        20,
                        method,
                        criteria,
                        144,
                        &options,
                        null_mut()
                    ),
                    0
                );
                let mut actual = [0.0; 8];
                assert_eq!(
                    itofin_bates_model_params(
                        &mut context,
                        model,
                        actual.as_mut_ptr(),
                        8,
                        null_mut()
                    ),
                    0
                );
                assert_eq!(actual, params);
            }
            let invalid_flags = [2_u8; 8];
            let options = ItofinCalibrationOptions {
                constraint: 0,
                weights: std::ptr::null(),
                weights_len: 0,
                fix_parameters: invalid_flags.as_ptr(),
                fix_parameters_len: 8,
            };
            assert_ne!(
                itofin_bates_calibrate_with_options(
                    &mut context,
                    model,
                    helper_ids.as_ptr(),
                    20,
                    method,
                    criteria,
                    144,
                    &options,
                    null_mut()
                ),
                0
            );
            for (ids, len, order) in [(std::ptr::null(), 0, 144), (helper_ids.as_ptr(), 20, 0)] {
                assert_ne!(
                    itofin_bates_calibrate_with_options(
                        &mut context,
                        model,
                        ids,
                        len,
                        method,
                        criteria,
                        order,
                        std::ptr::null(),
                        null_mut()
                    ),
                    0
                );
            }
        }
    }
}
