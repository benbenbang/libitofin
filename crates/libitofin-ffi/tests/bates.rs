use itofin_ffi::bates_api::*;
use itofin_ffi::boundary::{Context, INVALID_HANDLE, itofin_handle_release};
use itofin_ffi::joint_curves_api::itofin_flat_forward_from_quote;
use itofin_ffi::market_api::{itofin_quote_new, itofin_quote_set};
use itofin_ffi::options_api::{
    itofin_option_calculate, itofin_option_new, itofin_option_set_engine, itofin_option_value,
};

use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use std::ptr::null_mut;

struct Market {
    context: Context,
    quotes: [u64; 3],
    curves: [u64; 2],
    process: u64,
    model: u64,
    engine: u64,
    option: u64,
    expiry: i32,
}
impl Market {
    fn new() -> Self {
        let mut context = Context::new();
        let today = Date::new(2, Month::October, 2026);
        let expiry = today.serial_number() + 365;
        let dc = context.insert(Actual365Fixed::new()).unwrap();
        let mut quotes = [0; 3];
        let mut curves = [0; 2];
        let mut process = 0;
        let mut model = 0;
        let mut engine = 0;
        let mut option = 0;
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settings = context.insert(settings).unwrap();
        unsafe {
            for (id, value) in quotes.iter_mut().zip([100.0, 0.05, 0.02]) {
                assert_eq!(itofin_quote_new(&mut context, value, id, null_mut()), 0);
            }
            for i in 0..2 {
                assert_eq!(
                    itofin_flat_forward_from_quote(
                        &mut context,
                        today.serial_number(),
                        quotes[i + 1],
                        dc,
                        &mut curves[i],
                        null_mut()
                    ),
                    0
                );
            }
            let parameters = parameters();
            assert_eq!(
                itofin_bates_process_new(
                    &mut context,
                    quotes[0],
                    curves[0],
                    curves[1],
                    &parameters,
                    &mut process,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_bates_model_new(&mut context, process, &mut model, null_mut()),
                0
            );
            assert_eq!(
                itofin_bates_engine_new(&mut context, model, 144, &mut engine, null_mut()),
                0
            );
            assert_eq!(
                itofin_option_new(
                    &mut context,
                    0,
                    100.0,
                    0,
                    expiry,
                    0,
                    settings,
                    &mut option,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_option_set_engine(&mut context, option, engine, 2, 0, null_mut()),
                0
            );
        }
        Self {
            context,
            quotes,
            curves,
            process,
            model,
            engine,
            option,
            expiry,
        }
    }
    fn value(&mut self) -> f64 {
        let mut value = 0.0;
        assert_eq!(
            unsafe {
                itofin_option_value(&mut self.context, self.option, 0, &mut value, null_mut())
            },
            0
        );
        value
    }
    fn set_quote(&mut self, index: usize, value: f64) {
        assert_eq!(
            unsafe { itofin_quote_set(&mut self.context, self.quotes[index], value, null_mut()) },
            0
        );
    }
}

fn parameters() -> ItofinBatesParameters {
    ItofinBatesParameters {
        v0: 0.04,
        kappa: 1.5,
        theta: 0.04,
        sigma: 0.3,
        rho: -0.7,
        lambda: 0.5,
        nu: -0.1,
        delta: 0.2,
    }
}

#[test]
fn bates_live_inputs_parameters_and_retained_graph() {
    let mut m = Market::new();
    let baseline = m.value();
    assert!(baseline.is_finite() && baseline > 0.0);
    unsafe {
        let expected = [0.04, 1.5, 0.04, 0.3, -0.7, 0.5, -0.1, 0.2];
        for kind in 0..=1 {
            let handle = if kind == 0 { m.process } else { m.model };
            for (field, want) in expected.into_iter().enumerate() {
                let mut value = -999.0;
                assert_eq!(
                    itofin_bates_parameter(
                        &mut m.context,
                        handle,
                        kind,
                        field,
                        &mut value,
                        null_mut()
                    ),
                    0
                );
                assert_eq!(value, want);
            }
        }
        let mut initial = [-999.0; 2];
        assert_eq!(
            itofin_bates_process_initial_values(
                &mut m.context,
                m.process,
                initial.as_mut_ptr(),
                2,
                null_mut()
            ),
            0
        );
        assert_eq!(initial, [100.0, 0.04]);
        let mut time = 0.0;
        assert_eq!(
            itofin_bates_process_time(&mut m.context, m.process, m.expiry, &mut time, null_mut()),
            0
        );
        assert_eq!(time, 1.0);
        let mut params = [0.0; 8];
        assert_eq!(
            itofin_bates_model_params(&mut m.context, m.model, params.as_mut_ptr(), 8, null_mut()),
            0
        );
        assert_eq!(params, [0.04, 1.5, 0.3, -0.7, 0.04, -0.1, 0.2, 0.5]);
        let original = params;
        params[7] = 0.9;
        assert_eq!(
            itofin_bates_model_set_params(&mut m.context, m.model, params.as_ptr(), 8, null_mut()),
            0
        );
        assert!((m.value() - baseline).abs() > 1e-6);
        assert_eq!(
            itofin_bates_model_set_params(
                &mut m.context,
                m.model,
                original.as_ptr(),
                8,
                null_mut()
            ),
            0
        );
        assert_eq!(m.value(), baseline);
    }
    for (index, (old, new)) in [(100.0, 105.0), (0.05, 0.06), (0.02, 0.03)]
        .into_iter()
        .enumerate()
    {
        m.set_quote(index, new);
        let mut calculated = 1;
        assert_eq!(
            unsafe {
                itofin_option_calculate(&mut m.context, m.option, 0, &mut calculated, null_mut())
            },
            0
        );
        assert_eq!(calculated, 0);
        assert!((m.value() - baseline).abs() > 1e-6);
        m.set_quote(index, old);
        assert_eq!(m.value(), baseline);
    }
    unsafe {
        for id in m
            .quotes
            .into_iter()
            .chain(m.curves)
            .chain([m.process, m.model, m.engine])
        {
            assert_eq!(itofin_handle_release(&mut m.context, id, null_mut()), 0);
        }
    }
    assert_eq!(m.value(), baseline);
    let mut value = 123.0;
    assert_eq!(
        unsafe { itofin_bates_parameter(&mut m.context, m.process, 0, 0, &mut value, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(value, 123.0);
}
