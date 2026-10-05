use super::*;
#[path = "variance_swap_api_test_helpers.rs"]
mod helpers;
use helpers::Market;

#[test]
fn typed_terms_live_spot_retained_sources_and_cache_recovery() {
    let mut m = Market::new();
    assert_eq!(m.integer(0), 0);
    assert_eq!(m.integer(1), m.today.serial_number());
    assert_eq!(m.integer(2), m.maturity.serial_number());
    assert_eq!(m.integer(3), 0);
    assert_eq!(m.value(2), 0.04);
    assert_eq!(m.value(3), 1000.);
    let variance = m.value(1);
    let npv = m.value(0);
    assert!((npv - (-0.05_f64).exp() * 1000. * (variance - 0.04)).abs() < 1e-12);
    assert_eq!(m.integer(3), 1);
    m.spot.set_value(105.);
    assert_eq!(m.integer(3), 0);
    assert_ne!(m.value(1), variance);
    m.spot.set_value(100.);
    assert_eq!(m.value(1), variance);
    m.spot.set_value(f64::NAN);
    let mut out = 91.;
    assert_ne!(
        unsafe { itofin_variance_swap_value(&mut m.c, m.swap, 1, &mut out, std::ptr::null_mut()) },
        0
    );
    assert_eq!(out, 91.);
    assert_eq!(m.integer(3), 0);
    m.spot.set_value(100.);
    assert_eq!(m.value(1), variance);
    for id in [m.engine, m.process, m.settings_id] {
        assert_eq!(
            unsafe { itofin_handle_release(&mut m.c, id, std::ptr::null_mut()) },
            0
        );
    }
    assert_eq!(
        unsafe { itofin_variance_swap_recalculate(&mut m.c, m.swap, std::ptr::null_mut()) },
        0
    );
    assert_eq!(m.value(1), variance);
    m.settings.set_evaluation_date(m.maturity + 1);
    assert_eq!(m.integer(4), 1);
    assert_eq!(m.value(0), 0.);
    assert_ne!(
        unsafe { itofin_variance_swap_value(&mut m.c, m.swap, 1, &mut out, std::ptr::null_mut()) },
        0
    );
}

#[test]
fn exact_snapshot_lengths_types_and_engine_replacement() {
    let mut m = Market::new();
    let mut count = 91;
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights_count(&mut m.c, m.swap, &mut count, std::ptr::null_mut())
        },
        0
    );
    assert_eq!(count, 6);
    let mut kinds = [-91; 6];
    let mut strikes = [91.; 6];
    let mut weights = [91.; 6];
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights(
                &mut m.c,
                m.swap,
                kinds.as_mut_ptr(),
                strikes.as_mut_ptr(),
                weights.as_mut_ptr(),
                6,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(kinds, [0, 0, 0, 1, 1, 1]);
    assert_eq!(strikes, [100., 110., 120., 100., 90., 80.]);
    assert!(weights.iter().all(|v| v.is_finite()));
    weights[0] = 91.;
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights(
                &mut m.c,
                m.swap,
                kinds.as_mut_ptr(),
                strikes.as_mut_ptr(),
                weights.as_mut_ptr(),
                6,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_ne!(weights[0], 91.);
    let calls = [100., 105., 110., 120., 120.];
    let puts = [80., 90., 100., 100.];
    let mut engine = 0;
    assert_eq!(
        unsafe {
            itofin_replicating_variance_swap_engine_new(
                &mut m.c,
                m.process,
                5.,
                calls.as_ptr(),
                calls.len(),
                puts.as_ptr(),
                puts.len(),
                &mut engine,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(
        unsafe { itofin_variance_swap_set_engine(&mut m.c, m.swap, engine, std::ptr::null_mut()) },
        0
    );
    assert_eq!(m.integer(3), 0);
    kinds.fill(-91);
    strikes.fill(91.);
    weights.fill(91.);
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights(
                &mut m.c,
                m.swap,
                kinds.as_mut_ptr(),
                strikes.as_mut_ptr(),
                weights.as_mut_ptr(),
                6,
                std::ptr::null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(m.integer(3), 0);
    assert_eq!(weights, [91.; 6]);
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights_count(&mut m.c, m.swap, &mut count, std::ptr::null_mut())
        },
        0
    );
    assert_eq!(count, 7);
}

#[test]
fn malformed_pointers_errors_bounds_and_wrong_handles_do_not_mutate() {
    let mut m = Market::new();
    let foreign = Context::new().insert(shared_mut(1_u32)).unwrap();
    for id in [m.process, m.settings_id, foreign, 0] {
        assert_eq!(
            unsafe { itofin_variance_swap_set_engine(&mut m.c, m.swap, id, std::ptr::null_mut()) },
            INVALID_HANDLE
        );
    }
    assert_eq!(m.integer(3), 0);
    let bad_error = std::ptr::without_provenance_mut::<ItofinError>(1);
    assert_eq!(
        unsafe { itofin_variance_swap_recalculate(&mut m.c, m.swap, bad_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_variance_swap_set_engine(&mut m.c, m.swap, m.engine, bad_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(m.integer(3), 0);
    for out in [
        std::ptr::null_mut(),
        std::ptr::without_provenance_mut::<f64>(1),
    ] {
        assert_eq!(
            unsafe { itofin_variance_swap_value(&mut m.c, m.swap, 0, out, std::ptr::null_mut()) },
            INVALID_ARGUMENT
        );
    }
    assert_eq!(m.integer(3), 0);
    let mut out = 91;
    assert_eq!(
        unsafe {
            itofin_replicating_variance_swap_engine_new(
                &mut m.c,
                m.process,
                5.,
                std::ptr::null(),
                4097,
                std::ptr::null(),
                2,
                &mut out,
                std::ptr::null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91);
    let calls = [100., 110.];
    let puts = [90., 100.];
    for dk in [0., -1., f64::NAN, f64::INFINITY, 100.] {
        assert_ne!(
            unsafe {
                itofin_replicating_variance_swap_engine_new(
                    &mut m.c,
                    m.process,
                    dk,
                    calls.as_ptr(),
                    2,
                    puts.as_ptr(),
                    2,
                    &mut out,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_eq!(out, 91);
    }
    for field in [-1, 4, 99] {
        let mut scalar = 91.;
        assert_eq!(
            unsafe {
                itofin_variance_swap_value(
                    &mut m.c,
                    m.swap,
                    field,
                    &mut scalar,
                    std::ptr::null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(scalar, 91.);
    }
    for field in [-1, 5, 99] {
        let mut scalar = 91;
        assert_eq!(
            unsafe {
                itofin_variance_swap_integer(
                    &mut m.c,
                    m.swap,
                    field,
                    &mut scalar,
                    std::ptr::null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(scalar, 91);
    }
}

#[test]
fn weight_pointer_rejection_and_expiry_reference_date_semantics() {
    let mut m = Market::new();
    let mut kinds = [-91; 6];
    let mut strikes = [91.; 6];
    let mut weights = [91.; 6];
    for invalid in [
        std::ptr::null_mut(),
        std::ptr::without_provenance_mut::<f64>(1),
    ] {
        assert_eq!(
            unsafe {
                itofin_variance_swap_weights(
                    &mut m.c,
                    m.swap,
                    kinds.as_mut_ptr(),
                    strikes.as_mut_ptr(),
                    invalid,
                    6,
                    std::ptr::null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(m.integer(3), 0);
        assert_eq!(weights, [91.; 6]);
    }
    let error = std::ptr::without_provenance_mut::<ItofinError>(1);
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights(
                &mut m.c,
                m.swap,
                kinds.as_mut_ptr(),
                strikes.as_mut_ptr(),
                weights.as_mut_ptr(),
                6,
                error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(m.integer(3), 0);
    let mut sentinel = 91;
    assert_eq!(
        unsafe {
            itofin_variance_swap_new(
                &mut m.c,
                0,
                0.04,
                1000.,
                m.today.serial_number(),
                m.maturity.serial_number(),
                m.settings_id,
                &mut sentinel,
                error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(sentinel, 91);
    m.settings.set_evaluation_date(m.maturity);
    assert_eq!(m.integer(4), 1);
    assert_eq!(m.value(0), 0.);
    m.settings.set_include_reference_date_events(true);
    assert_eq!(m.integer(4), 0);
    assert_ne!(
        unsafe { itofin_variance_swap_recalculate(&mut m.c, m.swap, std::ptr::null_mut()) },
        0
    );
    m.settings.set_evaluation_date(m.today);
    assert_eq!(m.integer(4), 0);
    assert!(m.value(1).is_finite());
}
