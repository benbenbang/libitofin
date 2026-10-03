//! Independent QuantLib 1.43 numerical expectations are documented in
//! `sdk/go/testdata/gjrgarch-model-oracle.md` and its tracked JSON fixtures.
use super::*;
use itofin_ffi::ItofinError;
use itofin_ffi::boundary::{CORE_ERROR, INVALID_ARGUMENT, INVALID_HANDLE};
use itofin_ffi::gjr_model_api::mc::*;

fn config() -> ItofinGjrMcConfig {
    ItofinGjrMcConfig {
        steps: 12,
        samples: 4096,
        seed: 42,
        antithetic: 1,
        ..ItofinGjrMcConfig::default()
    }
}

#[test]
fn seeded_mc_prices_error_estimate_and_live_repricing() {
    let mut m = Market::new();
    let mut engine = 0;
    assert_eq!(
        unsafe {
            itofin_gjr_mc_engine_new(
                &mut m.context,
                m.process,
                &config(),
                &mut engine,
                null_mut(),
            )
        },
        0
    );
    m.attach(engine);
    let first = m.value();
    let mut stderr = -999.0;
    assert_eq!(
        unsafe { itofin_option_value(&mut m.context, m.option, 7, &mut stderr, null_mut()) },
        0
    );
    assert!(first > 0.0 && stderr.is_finite() && stderr > 0.0);
    let mut second_engine = 0;
    assert_eq!(
        unsafe {
            itofin_gjr_mc_engine_new(
                &mut m.context,
                m.process,
                &config(),
                &mut second_engine,
                null_mut(),
            )
        },
        0
    );
    m.attach(second_engine);
    assert_eq!(m.value(), first);
    assert_eq!(
        unsafe { itofin_quote_set(&mut m.context, m.quotes[0], 110.0, null_mut()) },
        0
    );
    assert!(m.value() > first);
    assert_eq!(
        unsafe { itofin_quote_set(&mut m.context, m.quotes[0], 100.0, null_mut()) },
        0
    );
    assert_eq!(m.value(), first);
    let tolerance = ItofinGjrMcConfig {
        steps_per_year: 12,
        absolute_tolerance: 1.0,
        max_samples: 4096,
        seed: 42,
        antithetic: 1,
        ..ItofinGjrMcConfig::default()
    };
    assert_eq!(
        unsafe {
            itofin_gjr_mc_engine_new(
                &mut m.context,
                m.process,
                &tolerance,
                &mut engine,
                null_mut(),
            )
        },
        0
    );
    m.attach(engine);
    assert!(m.value() > 0.0);
    assert_eq!(
        unsafe { itofin_option_value(&mut m.context, m.option, 7, &mut stderr, null_mut()) },
        0
    );
    assert!(stderr <= 1.0);
}

#[test]
fn mc_rejects_conflicts_overflow_bad_flags_and_keeps_output() {
    let mut m = Market::new();
    let base = config();
    let invalid = [
        ItofinGjrMcConfig { steps: 0, ..base },
        ItofinGjrMcConfig {
            steps_per_year: 12,
            ..base
        },
        ItofinGjrMcConfig { samples: 0, ..base },
        ItofinGjrMcConfig {
            absolute_tolerance: 0.1,
            ..base
        },
        ItofinGjrMcConfig {
            absolute_tolerance: f64::NAN,
            ..base
        },
        ItofinGjrMcConfig {
            absolute_tolerance: -1.0,
            ..base
        },
        ItofinGjrMcConfig {
            max_samples: 1,
            ..base
        },
        ItofinGjrMcConfig {
            seed: u64::MAX,
            ..base
        },
        ItofinGjrMcConfig {
            antithetic: 2,
            ..base
        },
    ];
    let mut out = 999;
    for cfg in invalid {
        assert_eq!(
            unsafe {
                itofin_gjr_mc_engine_new(&mut m.context, m.process, &cfg, &mut out, null_mut())
            },
            INVALID_ARGUMENT
        );
        assert_eq!(out, 999);
    }
    for cfg in [
        ItofinGjrMcConfig {
            steps: usize::MAX,
            ..base
        },
        ItofinGjrMcConfig { samples: 1, ..base },
        ItofinGjrMcConfig {
            samples: usize::MAX,
            ..base
        },
        ItofinGjrMcConfig {
            steps: 0,
            steps_per_year: 12,
            samples: 0,
            absolute_tolerance: 1.0,
            max_samples: 2,
            ..base
        },
    ] {
        assert_eq!(
            unsafe {
                itofin_gjr_mc_engine_new(&mut m.context, m.process, &cfg, &mut out, null_mut())
            },
            CORE_ERROR
        );
        assert_eq!(out, 999);
    }
    assert_eq!(
        unsafe { itofin_gjr_mc_engine_new(&mut m.context, m.model, &base, &mut out, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(out, 999);
    assert_eq!(
        unsafe {
            itofin_gjr_mc_engine_new(
                &mut m.context,
                m.process,
                std::ptr::null(),
                &mut out,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    let mut storage = [0_u64; 140];
    let misaligned = unsafe { storage.as_mut_ptr().cast::<u8>().add(1) };
    assert_eq!(
        unsafe {
            itofin_gjr_mc_engine_new(
                &mut m.context,
                m.process,
                misaligned.cast::<ItofinGjrMcConfig>(),
                &mut out,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe {
            itofin_gjr_mc_engine_new(
                &mut m.context,
                m.process,
                &base,
                &mut out,
                misaligned.cast::<ItofinError>(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 999);
}

#[test]
fn seeded_reflection_price_matches_independent_quantlib_fixture() {
    let mut m = Market::new();
    let today = Date::new(3, Month::October, 2026);
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);
    let settings = m.context.insert(settings).unwrap();
    let mut process = 0;
    assert_eq!(
        unsafe {
            itofin_gjr_process_new(
                &mut m.context,
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                &ItofinGjrParameters {
                    daily_variance: 0.002,
                    omega: 2e-6,
                    alpha: 0.3,
                    beta: 0.4,
                    gamma: 0.9,
                    lambda: -0.4,
                    days_per_year: 252.0,
                },
                2,
                &mut process,
                null_mut(),
            )
        },
        0
    );
    let mut option = 0;
    assert_eq!(
        unsafe {
            itofin_option_new(
                &mut m.context,
                0,
                100.0,
                0,
                today.serial_number() + 7,
                0,
                settings,
                &mut option,
                null_mut(),
            )
        },
        0
    );
    m.option = option;
    for (antithetic, price, error) in [
        (0, 3.884892840856974, 0.14498983462806309),
        (1, 4.024609001698537, 0.0841233780095511),
    ] {
        let cfg = ItofinGjrMcConfig {
            steps: 4,
            samples: 2048,
            seed: 42,
            antithetic,
            ..ItofinGjrMcConfig::default()
        };
        let mut engine = 0;
        assert_eq!(
            unsafe {
                itofin_gjr_mc_engine_new(&mut m.context, process, &cfg, &mut engine, null_mut())
            },
            0
        );
        m.attach(engine);
        assert!((m.value() - price).abs() < 2e-12);
        let mut stderr = -999.0;
        assert_eq!(
            unsafe { itofin_option_value(&mut m.context, m.option, 7, &mut stderr, null_mut()) },
            0
        );
        assert!((stderr - error).abs() < 2e-12);
    }
}
