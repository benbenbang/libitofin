use super::super::tests::helpers::Market;
use super::*;

fn config() -> ItofinVarianceSwapMcConfig {
    ItofinVarianceSwapMcConfig {
        steps: 12,
        steps_per_year: 0,
        samples: 32,
        absolute_tolerance: 0.,
        max_samples: 0,
        seed: 42,
    }
}
fn attach(m: &mut Market, cfg: &ItofinVarianceSwapMcConfig) -> u64 {
    let mut id = 91;
    assert_eq!(
        unsafe {
            itofin_mc_variance_swap_engine_new(
                &mut m.c,
                m.process,
                cfg,
                &mut id,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(
        unsafe { itofin_variance_swap_set_engine(&mut m.c, m.swap, id, std::ptr::null_mut()) },
        0
    );
    id
}
fn samples(m: &mut Market) -> usize {
    let mut n = 91;
    assert_eq!(
        unsafe { itofin_variance_swap_samples(&mut m.c, m.swap, &mut n, std::ptr::null_mut()) },
        0
    );
    n
}

#[test]
fn mc_statistics_empty_weights_retention_and_replacement() {
    let mut m = Market::new();
    let engine = attach(&mut m, &config());
    assert_eq!(m.integer(1), m.today.serial_number());
    assert!((m.value(1) - 0.04).abs() < 1e-14);
    assert!(m.value(4).abs() < 1e-12);
    assert!(m.value(5) < 1e-14);
    assert_eq!(samples(&mut m), 32);
    m.spot.set_value(105.);
    assert_eq!(m.integer(3), 0);
    assert!((m.value(1) - 0.04).abs() < 1e-14);
    let mut count = 91;
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights_count(&mut m.c, m.swap, &mut count, std::ptr::null_mut())
        },
        0
    );
    assert_eq!(count, 0);
    assert_eq!(
        unsafe {
            itofin_variance_swap_weights(
                &mut m.c,
                m.swap,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(
        unsafe {
            itofin_variance_swap_set_engine(&mut m.c, m.swap, m.engine, std::ptr::null_mut())
        },
        0
    );
    let mut n = 91;
    assert_ne!(
        unsafe { itofin_variance_swap_samples(&mut m.c, m.swap, &mut n, std::ptr::null_mut()) },
        0
    );
    assert_eq!(n, 91);
    assert_eq!(
        unsafe { itofin_variance_swap_set_engine(&mut m.c, m.swap, engine, std::ptr::null_mut()) },
        0
    );
    let variance = m.value(1);
    for id in [engine, m.process, m.settings_id] {
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
    assert_eq!(samples(&mut m), 32);
    m.settings.set_evaluation_date(m.maturity);
    assert_eq!(m.value(0), 0.);
    assert_eq!(m.value(4), 0.);
    let mut out = 91.;
    assert_ne!(
        unsafe { itofin_variance_swap_value(&mut m.c, m.swap, 5, &mut out, std::ptr::null_mut()) },
        0
    );
    assert_eq!(out, 91.);
    assert_ne!(
        unsafe { itofin_variance_swap_samples(&mut m.c, m.swap, &mut n, std::ptr::null_mut()) },
        0
    );
    assert_eq!(n, 91);
}

#[test]
fn mc_constructor_rejects_invalid_configuration_without_output_mutation() {
    let mut m = Market::new();
    let base = config();
    let invalid = [
        ItofinVarianceSwapMcConfig { steps: 0, ..base },
        ItofinVarianceSwapMcConfig {
            steps_per_year: 12,
            ..base
        },
        ItofinVarianceSwapMcConfig { samples: 0, ..base },
        ItofinVarianceSwapMcConfig { samples: 1, ..base },
        ItofinVarianceSwapMcConfig {
            absolute_tolerance: 0.01,
            ..base
        },
        ItofinVarianceSwapMcConfig {
            samples: 0,
            absolute_tolerance: f64::NAN,
            ..base
        },
        ItofinVarianceSwapMcConfig {
            samples: 0,
            absolute_tolerance: -1.,
            ..base
        },
        ItofinVarianceSwapMcConfig {
            samples: 0,
            absolute_tolerance: f64::INFINITY,
            ..base
        },
        ItofinVarianceSwapMcConfig {
            samples: 0,
            absolute_tolerance: 0.01,
            max_samples: 1022,
            ..base
        },
        ItofinVarianceSwapMcConfig {
            steps: 100001,
            ..base
        },
        ItofinVarianceSwapMcConfig {
            samples: 1000001,
            ..base
        },
        ItofinVarianceSwapMcConfig {
            max_samples: 1,
            ..base
        },
    ];
    for cfg in invalid {
        let mut out = 91;
        assert_ne!(
            unsafe {
                itofin_mc_variance_swap_engine_new(
                    &mut m.c,
                    m.process,
                    &cfg,
                    &mut out,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_eq!(out, 91);
    }
    let cfg = ItofinVarianceSwapMcConfig {
        steps: 0,
        steps_per_year: 12,
        samples: 0,
        absolute_tolerance: 0.01,
        ..base
    };
    attach(&mut m, &cfg);
    assert_eq!(samples(&mut m), 1023);
}

#[test]
fn mc_pointer_handle_and_thread_errors_are_atomic() {
    let mut m = Market::new();
    let cfg = config();
    let mut out = 91;
    let mut foreign = Context::new();
    let foreign_id = foreign.insert(1_u32).unwrap();
    for id in [0, m.swap, m.engine, m.settings_id, foreign_id] {
        assert_ne!(
            unsafe {
                itofin_mc_variance_swap_engine_new(
                    &mut m.c,
                    id,
                    &cfg,
                    &mut out,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_eq!(out, 91);
    }
    assert_ne!(
        unsafe {
            itofin_mc_variance_swap_engine_new(
                &mut m.c,
                m.process,
                std::ptr::null(),
                &mut out,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(out, 91);
    assert_ne!(
        unsafe {
            itofin_mc_variance_swap_engine_new(
                &mut m.c,
                m.process,
                &cfg,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    let bytes = [0_u64; 16];
    let misaligned =
        unsafe { bytes.as_ptr().cast::<u8>().add(1) }.cast::<ItofinVarianceSwapMcConfig>();
    assert_ne!(
        unsafe {
            itofin_mc_variance_swap_engine_new(
                &mut m.c,
                m.process,
                misaligned,
                &mut out,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(out, 91);
    let engine = attach(&mut m, &cfg);
    assert_eq!(
        unsafe { itofin_handle_release(&mut m.c, engine, std::ptr::null_mut()) },
        0
    );
    assert_ne!(
        unsafe { itofin_variance_swap_set_engine(&mut m.c, m.swap, engine, std::ptr::null_mut()) },
        0
    );
    assert_eq!(samples(&mut m), 32);
    let address = (&mut m.c as *mut Context) as usize;
    let swap = m.swap;
    let result = std::thread::spawn(move || {
        let mut n = 91;
        let status = unsafe {
            itofin_variance_swap_samples(
                address as *mut Context,
                swap,
                &mut n,
                std::ptr::null_mut(),
            )
        };
        (status, n)
    })
    .join()
    .unwrap();
    assert_ne!(result.0, 0);
    assert_eq!(result.1, 91);
    assert_ne!(
        unsafe {
            itofin_variance_swap_samples(
                &mut m.c,
                m.swap,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
}

#[test]
fn mc_sample_output_rejects_wrong_foreign_and_misaligned_handles() {
    let mut m = Market::new();
    attach(&mut m, &config());
    let mut foreign = Context::new();
    let foreign_id = foreign.insert(1_u32).unwrap();
    let mut n = 91;
    for id in [0, m.process, m.engine, m.settings_id, foreign_id] {
        assert_ne!(
            unsafe { itofin_variance_swap_samples(&mut m.c, id, &mut n, std::ptr::null_mut()) },
            0
        );
        assert_eq!(n, 91);
    }
    let mut aligned = [91_u64; 2];
    let p = unsafe { aligned.as_mut_ptr().cast::<u8>().add(1) }.cast::<usize>();
    assert_ne!(
        unsafe { itofin_variance_swap_samples(&mut m.c, m.swap, p, std::ptr::null_mut()) },
        0
    );
    assert_eq!(aligned, [91; 2]);
    assert_eq!(
        unsafe { itofin_handle_release(&mut m.c, m.swap, std::ptr::null_mut()) },
        0
    );
    assert_ne!(
        unsafe { itofin_variance_swap_samples(&mut m.c, m.swap, &mut n, std::ptr::null_mut()) },
        0
    );
    assert_eq!(n, 91);
}
