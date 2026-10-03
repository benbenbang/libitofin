//! Independent QuantLib 1.43 numerical expectations are documented in
//! `sdk/go/testdata/gjrgarch-model-oracle.md` and its tracked JSON fixtures.
use itofin_ffi::boundary::{Context, itofin_handle_release};
use itofin_ffi::gjr_api::{ItofinGjrParameters, itofin_gjr_process_new};
use itofin_ffi::gjr_model_api::*;
use itofin_ffi::joint_curves_api::itofin_flat_forward_from_quote;
use itofin_ffi::market_api::{itofin_quote_new, itofin_quote_set};
use itofin_ffi::options_api::{itofin_option_new, itofin_option_set_engine, itofin_option_value};
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
    dependencies: [u64; 2],
}
impl Market {
    fn new() -> Self {
        let mut context = Context::new();
        let today = Date::new(3, Month::October, 2026);
        let dc = context.insert(Actual365Fixed::new()).unwrap();
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settings = context.insert(settings).unwrap();
        let mut quotes = [0; 3];
        let mut curves = [0; 2];
        let (mut process, mut model, mut engine, mut option) = (0, 0, 0, 0);
        unsafe {
            for (id, value) in quotes.iter_mut().zip([100.0, 0.03, 0.01]) {
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
            assert_eq!(
                itofin_gjr_process_new(
                    &mut context,
                    quotes[0],
                    curves[0],
                    curves[1],
                    &ItofinGjrParameters {
                        daily_variance: 0.04 / 252.0,
                        omega: 2e-6,
                        alpha: 0.04,
                        beta: 0.88,
                        gamma: 0.08,
                        lambda: -0.4,
                        days_per_year: 252.0,
                    },
                    2,
                    &mut process,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_gjr_model_new(&mut context, process, &mut model, null_mut()),
                0
            );
            assert_eq!(
                itofin_gjr_analytic_engine_new(&mut context, model, &mut engine, null_mut()),
                0
            );
            assert_eq!(
                itofin_option_new(
                    &mut context,
                    0,
                    100.0,
                    0,
                    today.serial_number() + 365,
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
            dependencies: [dc, settings],
        }
    }
    fn params(&mut self) -> [f64; 6] {
        let mut values = [0.0; 6];
        assert_eq!(
            unsafe {
                itofin_gjr_model_params(
                    &mut self.context,
                    self.model,
                    values.as_mut_ptr(),
                    6,
                    null_mut(),
                )
            },
            0
        );
        values
    }
    fn value(&mut self) -> f64 {
        let mut value = -999.0;
        assert_eq!(
            unsafe {
                itofin_option_value(&mut self.context, self.option, 0, &mut value, null_mut())
            },
            0
        );
        value
    }
    fn attach(&mut self, engine: u64) {
        assert_eq!(
            unsafe {
                itofin_option_set_engine(&mut self.context, self.option, engine, 2, 0, null_mut())
            },
            0
        );
    }
}

#[test]
fn model_parameter_order_snapshot_process_and_retained_graph() {
    use itofin_ffi::gjr_api::itofin_gjr_process_query;
    use itofin_ffi::options_api::itofin_option_calculate;
    use libitofin::quotes::SimpleQuote;
    use libitofin::shared::Shared;
    let mut m = Market::new();
    assert_eq!(m.params(), [2e-6, 0.04, 0.88, 0.08, -0.4, 0.04 / 252.0]);
    let baseline = m.value();
    assert!((baseline - 5.316599738704635).abs() < 1e-11);
    let mut snapshot = 0;
    assert_eq!(
        unsafe { itofin_gjr_model_process(&mut m.context, m.model, &mut snapshot, null_mut()) },
        0
    );
    let mut scheme = -1.0;
    assert_eq!(
        unsafe {
            itofin_gjr_process_query(
                &mut m.context,
                snapshot,
                4,
                0.0,
                std::ptr::null(),
                0,
                &mut scheme,
                1,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(scheme, 1.0);
    let mut updated = m.params();
    updated[5] *= 1.3;
    assert_eq!(
        unsafe {
            itofin_gjr_model_set_params(&mut m.context, m.model, updated.as_ptr(), 6, null_mut())
        },
        0
    );
    assert_eq!(m.params(), updated);
    assert_ne!(m.value(), baseline);
    let mut snapshot_params = [0.0; 7];
    assert_eq!(
        unsafe {
            itofin_gjr_process_query(
                &mut m.context,
                snapshot,
                3,
                0.0,
                std::ptr::null(),
                0,
                snapshot_params.as_mut_ptr(),
                7,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(snapshot_params[0], 0.04 / 252.0);
    assert_eq!(
        unsafe { itofin_quote_set(&mut m.context, m.quotes[0], 105.0, null_mut()) },
        0
    );
    let live = m.value();
    assert_ne!(live, baseline);
    let retained_spot = m.context.get::<Shared<SimpleQuote>>(m.quotes[0]).unwrap();
    for id in m.quotes[1..]
        .iter()
        .copied()
        .chain(m.curves)
        .chain(m.dependencies)
        .chain([m.process, m.model, m.engine, snapshot])
    {
        assert_eq!(
            unsafe { itofin_handle_release(&mut m.context, id, null_mut()) },
            0
        );
    }
    let mut calculated = -1;
    assert_eq!(
        unsafe {
            itofin_option_calculate(&mut m.context, m.option, 0, &mut calculated, null_mut())
        },
        0
    );
    assert_eq!(calculated, 1);
    retained_spot.set_value(110.0);
    assert_eq!(
        unsafe {
            itofin_option_calculate(&mut m.context, m.option, 0, &mut calculated, null_mut())
        },
        0
    );
    assert_eq!(calculated, 0);
    let mut independent = Market::new();
    assert_eq!(
        unsafe {
            itofin_gjr_model_set_params(
                &mut independent.context,
                independent.model,
                updated.as_ptr(),
                6,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(
        unsafe {
            itofin_quote_set(
                &mut independent.context,
                independent.quotes[0],
                110.0,
                null_mut(),
            )
        },
        0
    );
    let expected = independent.value();
    let repriced = m.value();
    assert_eq!(repriced, expected);
    assert_ne!(repriced, live);
    assert_eq!(
        unsafe {
            itofin_option_calculate(&mut m.context, m.option, 0, &mut calculated, null_mut())
        },
        0
    );
    assert_eq!(calculated, 1);
    assert_eq!(
        unsafe { itofin_handle_release(&mut m.context, m.quotes[0], null_mut()) },
        0
    );
}

#[path = "gjr_model/boundaries.rs"]
mod boundaries;
#[path = "gjr_model/calibration.rs"]
mod calibration;
#[path = "gjr_model/mc.rs"]
mod mc;
