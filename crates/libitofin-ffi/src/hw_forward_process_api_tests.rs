use super::*;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::patterns::observable::{AsObservable, Observer};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use std::cell::Cell;
use std::ptr::null_mut;

fn flat(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        Date::new(23, Month::October, 2026),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
fn create(c: &mut Context, curve: u64, a: f64, sigma: f64) -> u64 {
    let mut id = 99;
    assert_eq!(
        unsafe { itofin_hw_forward_process_new(c, curve, a, sigma, &mut id, null_mut()) },
        0
    );
    id
}
fn query(c: &mut Context, id: u64, kind: i32, t: f64, x: f64, dt: f64, dw: f64) -> f64 {
    let mut out = 99.;
    assert_eq!(
        unsafe { itofin_hw_forward_process_query(c, id, kind, t, x, dt, dw, &mut out, null_mut()) },
        0
    );
    out
}
fn near(got: f64, want: f64) {
    assert!((got - want).abs() <= 2e-12, "{got} != {want}");
}

#[test]
fn all_thirteen_dispatches_preserve_native_results_and_zero_speed_limit() {
    let mut c = Context::new();
    let curve = c.insert(Handle::new(flat(0.03))).unwrap();
    for a in [0., 1e-16, 1e-10, 0.2] {
        let id = create(&mut c, curve, a, 0.02);
        let mut native =
            HullWhiteForwardProcess::new(super::curve(&c, curve).unwrap(), a, 0.02).unwrap();
        assert_eq!(query(&mut c, id, 1, 0., 0., 0., 0.), a);
        assert_eq!(query(&mut c, id, 2, 0., 0., 0., 0.), 0.02);
        assert_eq!(query(&mut c, id, 3, 0., 0., 0., 0.), 0.);
        assert_eq!(
            unsafe {
                itofin_hw_forward_process_set_forward_measure_time(&mut c, id, 5., null_mut())
            },
            0
        );
        native.set_forward_measure_time(5.).unwrap();
        let expected = [
            native.x0().unwrap(),
            a,
            0.02,
            5.,
            native.alpha(1.).unwrap(),
            native.b(1., 0.8).unwrap(),
        ];
        for (kind, value) in expected.iter().enumerate().take(6) {
            assert_eq!(query(&mut c, id, kind as i32, 1., 0.03, 0.8, -0.7), *value);
        }
        assert_eq!(
            query(&mut c, id, 6, 1., 2., 5., 0.),
            native.m_t(1., 2., 5.).unwrap()
        );
        let values = [
            native.drift(1., 0.03).unwrap(),
            native.diffusion(1., 0.03).unwrap(),
            native.expectation(1., 0.03, 0.8).unwrap(),
            native.std_deviation(1., 0.03, 0.8).unwrap(),
            native.variance(1., 0.03, 0.8).unwrap(),
            native.evolve(1., 0.03, 0.8, -0.7).unwrap(),
        ];
        for (i, value) in values.iter().enumerate() {
            assert_eq!(
                query(&mut c, id, (i + 7) as i32, 1., 0.03, 0.8, -0.7),
                *value
            );
        }
        if a == 0. {
            near(query(&mut c, id, 4, 1., 0., 0., 0.), 0.0302);
            near(query(&mut c, id, 6, 1., 2., 5., 0.), 0.0014);
        }
    }
}

#[test]
fn exact_handle_survives_release_relinks_and_notifies_without_changing_x0() {
    let mut c = Context::new();
    let rate = shared(SimpleQuote::new(0.03));
    let curve = shared(FlatForward::new(
        Date::new(23, Month::October, 2026),
        Handle::new(rate.clone() as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let handle = RelinkableHandle::new(curve as Shared<dyn YieldTermStructure>);
    let curve_id = c.insert(handle.handle()).unwrap();
    let id = create(&mut c, curve_id, 0.1, 0.02);
    let x0 = query(&mut c, id, 0, 0., 0., 0., 0.);
    let alpha = query(&mut c, id, 4, 1., 0., 0., 0.);
    let current_forward = || {
        handle
            .handle()
            .current_link()
            .unwrap()
            .forward_rate(
                1.,
                1.,
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )
            .unwrap()
            .rate()
    };
    let before_forward = current_forward();
    assert_eq!(
        unsafe { itofin_handle_release(&mut c, curve_id, null_mut()) },
        0
    );
    rate.set_value(0.05);
    assert!(
        (query(&mut c, id, 4, 1., 0., 0., 0.) - alpha - (current_forward() - before_forward)).abs()
            <= 2e-16
    );
    assert_eq!(query(&mut c, id, 0, 0., 0., 0., 0.), x0);
    handle.link_to(flat(0.07));
    assert!(
        (query(&mut c, id, 4, 1., 0., 0., 0.) - alpha - (current_forward() - before_forward)).abs()
            <= 2e-16
    );
    assert_eq!(query(&mut c, id, 0, 0., 0., 0., 0.), x0);
    let process = c.get::<SharedMut<HullWhiteForwardProcess>>(id).unwrap();
    struct Counter(Shared<Cell<usize>>);
    impl Observer for Counter {
        fn update(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let count = shared(Cell::new(0));
    let listener = shared_mut(Counter(count.clone()));
    process
        .borrow()
        .observable()
        .register_observer(&(listener.clone() as _));
    assert_eq!(
        unsafe { itofin_hw_forward_process_set_forward_measure_time(&mut c, id, 5., null_mut()) },
        0
    );
    assert_eq!(count.get(), 1);
    assert_eq!(
        unsafe {
            itofin_hw_forward_process_set_forward_measure_time(&mut c, id, f64::NAN, null_mut())
        },
        CORE_ERROR
    );
    assert_eq!(count.get(), 1);
    handle.reset();
    let mut out = 99.;
    assert_eq!(
        unsafe {
            itofin_hw_forward_process_query(&mut c, id, 4, 1., 0., 0., 0., &mut out, null_mut())
        },
        CORE_ERROR
    );
    assert_eq!(out, 99.);
    assert_eq!(query(&mut c, id, 0, 0., 0., 0., 0.), x0);
    assert_eq!(query(&mut c, id, 8, 1., 0., 0., 0.), 0.02);
}

#[test]
fn error_and_pointer_guards_preserve_outputs_and_horizon() {
    let mut c = Context::new();
    let curve = c.insert(Handle::new(flat(0.03))).unwrap();
    let id = create(&mut c, curve, 0.1, 0.02);
    for (a, sigma) in [
        (-1., 0.02),
        (f64::NAN, 0.02),
        (0.1, f64::INFINITY),
        (0.1, -0.02),
    ] {
        let mut out = 99;
        assert_eq!(
            unsafe { itofin_hw_forward_process_new(&mut c, curve, a, sigma, &mut out, null_mut()) },
            CORE_ERROR
        );
        assert_eq!(out, 99);
    }
    for horizon in [-1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            unsafe {
                itofin_hw_forward_process_set_forward_measure_time(&mut c, id, horizon, null_mut())
            },
            CORE_ERROR
        );
        assert_eq!(query(&mut c, id, 3, 0., 0., 0., 0.), 0.);
    }
    for (kind, t, x, dt, dw) in [
        (99, 0., 0., 0., 0.),
        (4, -1., 0., 0., 0.),
        (5, 0., 0., f64::NAN, 0.),
        (6, 2., 1., 5., 0.),
        (7, 0., f64::INFINITY, 0., 0.),
        (8, f64::NAN, 0., 0., 0.),
        (9, 0., 0., -1., 0.),
        (10, 0., f64::NAN, 1., 0.),
        (11, 1e308, 0., 1e308, 0.),
        (12, 0., 0., 1., f64::NAN),
        (12, 0., 0.03, 0., f64::NAN),
        (11, 0., f64::NAN, 0., 0.),
    ] {
        let mut out = 99.;
        assert_ne!(
            unsafe {
                itofin_hw_forward_process_query(
                    &mut c,
                    id,
                    kind,
                    t,
                    x,
                    dt,
                    dw,
                    &mut out,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(out, 99.);
    }
    let bad_error = std::ptr::dangling_mut::<u8>().cast::<ItofinError>();
    assert_eq!(
        unsafe { itofin_hw_forward_process_set_forward_measure_time(&mut c, id, 5., bad_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(query(&mut c, id, 3, 0., 0., 0., 0.), 0.);
    let mut out = 99;
    assert_eq!(
        unsafe { itofin_hw_forward_process_new(&mut c, curve, 0.1, 0.02, &mut out, bad_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 99);
    assert_eq!(
        unsafe {
            itofin_hw_forward_process_query(&mut c, id, 0, 0., 0., 0., 0., null_mut(), null_mut())
        },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_hw_forward_process_new(&mut c, id, 0.1, 0.02, &mut out, null_mut()) },
        INVALID_HANDLE
    );
    let mut other = Context::new();
    let mut result = 99.;
    assert_eq!(
        unsafe {
            itofin_hw_forward_process_query(
                &mut other,
                id,
                0,
                0.,
                0.,
                0.,
                0.,
                &mut result,
                null_mut(),
            )
        },
        INVALID_HANDLE
    );
    assert_eq!(result, 99.);
    assert_eq!(unsafe { itofin_handle_release(&mut c, id, null_mut()) }, 0);
    assert_eq!(
        unsafe {
            itofin_hw_forward_process_query(&mut c, id, 0, 0., 0., 0., 0., &mut result, null_mut())
        },
        INVALID_HANDLE
    );
}

#[test]
fn wrong_thread_is_rejected_before_queries_or_mutation() {
    let mut c = Context::new();
    let curve = c.insert(Handle::new(flat(0.03))).unwrap();
    let id = create(&mut c, curve, 0.1, 0.02);
    let address = std::ptr::from_mut(&mut c) as usize;
    std::thread::spawn(move || {
        let ctx = address as *mut Context;
        let mut out = 99.;
        assert_eq!(
            unsafe {
                itofin_hw_forward_process_query(ctx, id, 0, 0., 0., 0., 0., &mut out, null_mut())
            },
            WRONG_THREAD
        );
        assert_eq!(out, 99.);
        assert_eq!(
            unsafe { itofin_hw_forward_process_set_forward_measure_time(ctx, id, 5., null_mut()) },
            WRONG_THREAD
        );
    })
    .join()
    .unwrap();
    assert_eq!(query(&mut c, id, 3, 0., 0., 0., 0.), 0.);
}
