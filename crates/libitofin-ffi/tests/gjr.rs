use itofin_ffi::boundary::{
    CORE_ERROR, Context, INVALID_ARGUMENT, INVALID_HANDLE, itofin_handle_release,
};
use itofin_ffi::gjr_api::*;
use itofin_ffi::joint_curves_api::itofin_flat_forward_from_quote;
use itofin_ffi::market_api::{itofin_quote_new, itofin_quote_set};
use libitofin::math::array::Array;
use libitofin::processes::GjrGarchProcess;
use libitofin::quotes::SimpleQuote;
use libitofin::shared::Shared;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use std::ptr::{null, null_mut};

fn parameters() -> ItofinGjrParameters {
    ItofinGjrParameters {
        daily_variance: 0.04 / 252.0,
        omega: 2e-6,
        alpha: 0.024,
        beta: 0.93,
        gamma: 0.059,
        lambda: 0.1,
        days_per_year: 252.0,
    }
}
struct Market {
    ctx: Context,
    quotes: [u64; 3],
    curves: [u64; 2],
    process: u64,
    reference: i32,
}
impl Market {
    fn new() -> Self {
        let mut ctx = Context::new();
        let reference = Date::new(3, Month::October, 2026).serial_number();
        let dc = ctx.insert(Actual365Fixed::new()).unwrap();
        let mut quotes = [0; 3];
        let mut curves = [0; 2];
        let mut process = 0;
        unsafe {
            for (id, value) in quotes.iter_mut().zip([100.0, 0.05, 0.02]) {
                assert_eq!(itofin_quote_new(&mut ctx, value, id, null_mut()), 0);
            }
            for i in 0..2 {
                assert_eq!(
                    itofin_flat_forward_from_quote(
                        &mut ctx,
                        reference,
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
                    &mut ctx,
                    quotes[0],
                    curves[0],
                    curves[1],
                    &parameters(),
                    1,
                    &mut process,
                    null_mut()
                ),
                0
            );
        }
        Self {
            ctx,
            quotes,
            curves,
            process,
            reference,
        }
    }
    fn query(&mut self, kind: i32, state: &[f64]) -> Vec<f64> {
        let mut out = vec![
            0.0;
            match kind {
                2 => 4,
                3 => 7,
                4 => 1,
                _ => 2,
            }
        ];
        assert_eq!(
            unsafe {
                itofin_gjr_process_query(
                    &mut self.ctx,
                    self.process,
                    kind,
                    0.0,
                    state.as_ptr(),
                    state.len(),
                    out.as_mut_ptr(),
                    out.len(),
                    null_mut(),
                )
            },
            0
        );
        out
    }
}

#[test]
fn direct_core_queries_and_evolution_are_bit_identical() {
    let mut m = Market::new();
    let core = m.ctx.get::<Shared<GjrGarchProcess>>(m.process).unwrap();
    let state = Array::from(vec![100.0, 0.04]);
    assert_eq!(m.query(0, &[]), core.initial_values().unwrap().to_vec());
    assert_eq!(
        m.query(1, &state),
        core.drift(0.0, &state).unwrap().to_vec()
    );
    let d = core.diffusion(0.0, &state).unwrap();
    assert_eq!(
        m.query(2, &state),
        [d[(0, 0)], d[(0, 1)], d[(1, 0)], d[(1, 1)]]
    );
    assert_eq!(m.query(4, &[]), [1.0]);
    let p = parameters();
    assert_eq!(
        m.query(3, &[]),
        [
            p.daily_variance,
            p.omega,
            p.alpha,
            p.beta,
            p.gamma,
            p.lambda,
            p.days_per_year
        ]
    );
    let draws = Array::from(vec![0.25, -0.5]);
    let mut out = [0.0; 2];
    assert_eq!(
        unsafe {
            itofin_gjr_process_evolve(
                &mut m.ctx,
                m.process,
                0.0,
                state.as_ptr(),
                2,
                1.0 / 252.0,
                draws.as_ptr(),
                2,
                out.as_mut_ptr(),
                2,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(
        out.to_vec(),
        core.evolve(0.0, &state, 1.0 / 252.0, &draws)
            .unwrap()
            .to_vec()
    );
    let mut time = -1.0;
    assert_eq!(
        unsafe {
            itofin_gjr_process_time(
                &mut m.ctx,
                m.process,
                m.reference + 365,
                &mut time,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(time, 1.0);
}

#[test]
fn market_inputs_remain_live_after_external_handles_release() {
    let mut m = Market::new();
    let state = [100.0, 0.04];
    let baseline = m.query(1, &state);
    let retained: Vec<_> = m
        .quotes
        .iter()
        .map(|&id| m.ctx.get::<Shared<SimpleQuote>>(id).unwrap())
        .collect();
    unsafe {
        assert_eq!(
            itofin_quote_set(&mut m.ctx, m.quotes[0], 105.0, null_mut()),
            0
        );
        assert_eq!(
            itofin_quote_set(&mut m.ctx, m.quotes[1], 0.06, null_mut()),
            0
        );
    }
    assert_eq!(m.query(0, &[])[0], 105.0);
    assert_ne!(m.query(1, &state), baseline);
    for id in m.quotes.into_iter().chain(m.curves) {
        assert_eq!(
            unsafe { itofin_handle_release(&mut m.ctx, id, null_mut()) },
            0
        );
    }
    retained[0].set_value(110.0);
    retained[1].set_value(0.07);
    retained[2].set_value(0.01);
    assert_eq!(m.query(0, &[])[0], 110.0);
    let updated = m.query(1, &state);
    assert_ne!(updated, baseline);
    assert!((updated[0] - (0.07 - 0.01 - 0.5 * 0.04)).abs() < 1e-12);
}

#[path = "gjr/boundaries.rs"]
mod boundaries;
