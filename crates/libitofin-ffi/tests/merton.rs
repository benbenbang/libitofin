use itofin_ffi::boundary::{
    CORE_ERROR, Context, INVALID_ARGUMENT, INVALID_HANDLE, itofin_handle_release,
};
use itofin_ffi::joint_curves_api::itofin_flat_forward_from_quote;
use itofin_ffi::market_api::{itofin_quote_new, itofin_quote_set};
use itofin_ffi::merton_api::*;
use itofin_ffi::options_api::{
    itofin_option_calculate, itofin_option_new, itofin_option_set_engine, itofin_option_value,
};
use itofin_ffi::vol_api::itofin_black_constant_vol_from_quote;
use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use std::ptr::null_mut;

struct Market {
    context: Context,
    quotes: [u64; 7],
    curves: [u64; 3],
    process: u64,
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
        let mut quotes = [0; 7];
        let mut curves = [0; 3];
        let mut process = 0;
        let mut engine = 0;
        let mut option = 0;
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settings = context.insert(settings).unwrap();
        unsafe {
            for (id, value) in quotes
                .iter_mut()
                .zip([100.0, 0.05, 0.02, 0.2, 1.0, -0.1, 0.3])
            {
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
                itofin_black_constant_vol_from_quote(
                    &mut context,
                    today.serial_number(),
                    quotes[3],
                    dc,
                    0,
                    &mut curves[2],
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_merton76_new(
                    &mut context,
                    quotes[0],
                    curves[0],
                    curves[1],
                    curves[2],
                    quotes[4],
                    quotes[5],
                    quotes[6],
                    &mut process,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_jump_diffusion_engine_new(
                    &mut context,
                    process,
                    1e-12,
                    1000,
                    &mut engine,
                    null_mut()
                ),
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

#[test]
fn all_market_updates_invalidate_and_handles_are_retained() {
    let mut market = Market::new();
    let baseline = market.value();
    assert!((baseline - 14.586_956_184_779_012).abs() < 1e-9);
    let mut value = 0.0;
    unsafe {
        for (field, expected) in [(0, 100.0), (1, 1.0), (2, -0.1), (3, 0.3)] {
            assert_eq!(
                itofin_merton76_value(
                    &mut market.context,
                    market.process,
                    field,
                    &mut value,
                    null_mut()
                ),
                0
            );
            assert_eq!(value, expected);
        }
        assert_eq!(
            itofin_merton76_time(
                &mut market.context,
                market.process,
                market.expiry,
                &mut value,
                null_mut()
            ),
            0
        );
        assert_eq!(value, 1.0);
    }
    for (index, (old, new)) in [
        (100.0, 105.0),
        (0.05, 0.06),
        (0.02, 0.03),
        (0.2, 0.25),
        (1.0, 1.3),
        (-0.1, -0.2),
        (0.3, 0.4),
    ]
    .into_iter()
    .enumerate()
    {
        market.set_quote(index, new);
        let mut valid = 1;
        assert_eq!(
            unsafe {
                itofin_option_calculate(
                    &mut market.context,
                    market.option,
                    0,
                    &mut valid,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(valid, 0);
        assert!((market.value() - baseline).abs() > 1e-7);
        market.set_quote(index, old);
        assert_eq!(market.value(), baseline);
    }
    unsafe {
        for id in market
            .quotes
            .into_iter()
            .chain(market.curves)
            .chain([market.process, market.engine])
        {
            assert_eq!(
                itofin_handle_release(&mut market.context, id, null_mut()),
                0
            );
        }
    }
    assert_eq!(market.value(), baseline);
}

#[test]
fn invalid_inputs_preserve_outputs_and_allow_recovery() {
    let mut m = Market::new();
    let mut id = 0;
    let mut value = 123.0;
    unsafe {
        assert_eq!(
            itofin_merton76_new(
                null_mut(),
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                m.curves[2],
                m.quotes[4],
                m.quotes[5],
                m.quotes[6],
                &mut id,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(id, 0);
        let mut foreign = Context::new();
        let mut foreign_quote = 0;
        assert_eq!(
            itofin_quote_new(&mut foreign, 100.0, &mut foreign_quote, null_mut()),
            0
        );
        for spot in [0, m.curves[0], foreign_quote] {
            assert_eq!(
                itofin_merton76_new(
                    &mut m.context,
                    spot,
                    m.curves[0],
                    m.curves[1],
                    m.curves[2],
                    m.quotes[4],
                    m.quotes[5],
                    m.quotes[6],
                    &mut id,
                    null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(id, 0);
        }
        assert_eq!(
            itofin_merton76_new(
                &mut m.context,
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                m.curves[2],
                m.quotes[4],
                m.quotes[5],
                m.quotes[6],
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        for (process, field) in [(m.quotes[0], 0), (m.process, 4)] {
            assert_ne!(
                itofin_merton76_value(&mut m.context, process, field, &mut value, null_mut()),
                0
            );
            assert_eq!(value, 123.0);
        }
        assert_eq!(
            itofin_merton76_value(&mut m.context, m.process, 0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_merton76_time(&mut m.context, m.process, 0, &mut value, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(value, 123.0);
        assert_eq!(
            itofin_merton76_time(&mut m.context, m.process, m.expiry, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_jump_diffusion_engine_new(
                &mut m.context,
                m.process,
                1e-4,
                100,
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        for (accuracy, iterations) in [
            (0.0, 100),
            (f64::NAN, 100),
            (f64::INFINITY, 100),
            (1e-4, 0),
            (1e-4, 100001),
        ] {
            assert_eq!(
                itofin_jump_diffusion_engine_new(
                    &mut m.context,
                    m.process,
                    accuracy,
                    iterations,
                    &mut id,
                    null_mut()
                ),
                CORE_ERROR
            );
            assert_eq!(id, 0);
        }
        assert_eq!(
            itofin_jump_diffusion_engine_new(
                &mut m.context,
                m.quotes[0],
                1e-4,
                100,
                &mut id,
                null_mut()
            ),
            INVALID_HANDLE
        );
        assert_eq!(id, 0);
    }
    for (index, bad, old) in [
        (0, 0.0, 100.0),
        (4, -1.0, 1.0),
        (5, f64::NAN, -0.1),
        (6, -0.1, 0.3),
        (3, f64::INFINITY, 0.2),
    ] {
        m.set_quote(index, bad);
        assert_eq!(
            unsafe { itofin_option_value(&mut m.context, m.option, 0, &mut value, null_mut()) },
            CORE_ERROR
        );
        assert_eq!(value, 123.0);
        m.set_quote(index, old);
        assert!(m.value().is_finite());
    }
    unsafe {
        assert_eq!(
            itofin_jump_diffusion_engine_new(
                &mut m.context,
                m.process,
                1e-14,
                1,
                &mut id,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_option_set_engine(&mut m.context, m.option, id, 2, 0, null_mut()),
            0
        );
        assert_eq!(
            itofin_option_value(&mut m.context, m.option, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, 123.0);
        assert_eq!(
            itofin_option_set_engine(&mut m.context, m.option, m.engine, 2, 0, null_mut()),
            0
        );
    }
    assert!(m.value().is_finite());
}
