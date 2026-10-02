use itofin_ffi::boundary::{CORE_ERROR, INVALID_ARGUMENT, ItofinError};
use itofin_ffi::merton_paths_api::{ItofinMertonInput, itofin_merton_paths};
use std::ptr::null_mut;

fn input() -> ItofinMertonInput {
    ItofinMertonInput {
        spot: 100.0,
        drift: 0.05,
        volatility: 0.2,
        jump_intensity: 1.0,
        log_mean_jump: -0.1,
        log_jump_volatility: 0.3,
        horizon: 1.0,
        steps: 4,
        paths: 3,
        seed: 42,
        terminal_only: 0,
    }
}
fn error() -> ItofinError {
    ItofinError {
        code: -1,
        message: [0; 1024],
    }
}
fn simulate(i: &ItofinMertonInput, out: &mut [f64], error: &mut ItofinError) -> i32 {
    unsafe { itofin_merton_paths(i, out.as_mut_ptr(), out.len(), error) }
}

#[test]
fn full_terminal_repeatability_and_zero_horizon() {
    let mut i = input();
    let mut full = [0.0; 15];
    let mut e = error();
    assert_eq!(simulate(&i, &mut full, &mut e), 0);
    assert_eq!(e.code, 0);
    assert_eq!(e.message[0], 0);
    let mut again = [0.0; 15];
    assert_eq!(simulate(&i, &mut again, &mut e), 0);
    assert_eq!(full, again);
    for path in 0..3 {
        assert_eq!(full[path * 5], 100.0);
    }
    i.terminal_only = 1;
    let mut terminal = [0.0; 3];
    assert_eq!(simulate(&i, &mut terminal, &mut e), 0);
    for path in 0..3 {
        assert_eq!(terminal[path].to_bits(), full[path * 5 + 4].to_bits());
    }
    i.horizon = 0.0;
    assert_eq!(simulate(&i, &mut terminal, &mut e), 0);
    assert_eq!(terminal, [100.0; 3]);
    i.terminal_only = 0;
    assert_eq!(simulate(&i, &mut full, &mut e), 0);
    assert_eq!(full, [100.0; 15]);
}

#[test]
fn pointer_capacity_and_flag_errors_preserve_output() {
    let mut i = input();
    let mut out = [91.0; 16];
    let mut e = error();
    unsafe {
        assert_eq!(
            itofin_merton_paths(std::ptr::null(), out.as_mut_ptr(), out.len(), &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 16]);
        assert_eq!(
            itofin_merton_paths(&i, out.as_mut_ptr(), 14, &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 16]);
        assert_eq!(
            itofin_merton_paths(&i, null_mut(), 15, &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 16]);
        let bad_error = (&raw mut e).cast::<u8>().add(1).cast::<ItofinError>();
        assert_eq!(
            itofin_merton_paths(&i, out.as_mut_ptr(), out.len(), bad_error),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 16]);
        let bad_input = (&raw const i)
            .cast::<u8>()
            .add(1)
            .cast::<ItofinMertonInput>();
        assert_eq!(
            itofin_merton_paths(bad_input, out.as_mut_ptr(), out.len(), &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 16]);
        let bad_output = out.as_mut_ptr().cast::<u8>().add(1).cast::<f64>();
        assert_eq!(
            itofin_merton_paths(&i, bad_output, out.len(), &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 16]);
    }
    i.terminal_only = 2;
    assert_eq!(simulate(&i, &mut out, &mut e), INVALID_ARGUMENT);
    assert_eq!(out, [91.0; 16]);
    i.terminal_only = 0;
    assert_eq!(
        unsafe { itofin_merton_paths(&i, out.as_mut_ptr(), out.len(), null_mut()) },
        0
    );
    assert_eq!(out[15], 91.0);
}

#[test]
fn domains_overflow_underflow_and_late_failure_leave_output_untouched() {
    let mut e = error();
    let mut out = [91.0; 15];
    let cases = [
        ItofinMertonInput {
            spot: 0.0,
            ..input()
        },
        ItofinMertonInput {
            drift: f64::NAN,
            ..input()
        },
        ItofinMertonInput {
            volatility: -1.0,
            ..input()
        },
        ItofinMertonInput {
            jump_intensity: -1.0,
            ..input()
        },
        ItofinMertonInput {
            log_mean_jump: f64::INFINITY,
            ..input()
        },
        ItofinMertonInput {
            log_jump_volatility: -1.0,
            ..input()
        },
        ItofinMertonInput {
            horizon: -1.0,
            ..input()
        },
        ItofinMertonInput {
            steps: 0,
            ..input()
        },
        ItofinMertonInput {
            paths: 0,
            ..input()
        },
        ItofinMertonInput { seed: 0, ..input() },
        ItofinMertonInput {
            steps: usize::MAX,
            terminal_only: 1,
            ..input()
        },
        ItofinMertonInput {
            paths: usize::MAX,
            ..input()
        },
        ItofinMertonInput {
            horizon: f64::from_bits(1),
            ..input()
        },
        ItofinMertonInput {
            jump_intensity: f64::from_bits(1),
            ..input()
        },
        ItofinMertonInput {
            jump_intensity: 1000.0,
            steps: 1,
            ..input()
        },
        ItofinMertonInput {
            volatility: 1e200,
            ..input()
        },
        ItofinMertonInput {
            log_mean_jump: -1000.0,
            ..input()
        },
        ItofinMertonInput {
            drift: -1000.0,
            volatility: 0.0,
            jump_intensity: 0.0,
            ..input()
        },
        ItofinMertonInput {
            log_jump_volatility: 1e200,
            ..input()
        },
        ItofinMertonInput {
            log_mean_jump: 1000.0,
            ..input()
        },
        ItofinMertonInput {
            drift: 1000.0,
            volatility: 0.0,
            jump_intensity: 0.0,
            ..input()
        },
    ];
    for i in cases {
        assert_eq!(simulate(&i, &mut out, &mut e), CORE_ERROR);
        assert_eq!(e.code, CORE_ERROR);
        assert_ne!(e.message[0], 0);
        assert_eq!(out, [91.0; 15]);
    }
    assert_eq!(simulate(&input(), &mut out, &mut e), 0);
    assert_eq!(e.code, 0);
}

#[test]
fn independent_base_and_multiple_jump_fixtures() {
    let cases = [
        (
            1.0,
            [
                100.0,
                55.496279986665456,
                61.56714406017473,
                73.98365027969385,
                69.02629897681246,
                100.0,
                108.62856072310466,
                119.8125070496225,
                111.58288619351339,
                116.76538287683597,
                100.0,
                92.29161151619682,
                154.0450892411835,
                142.169262692087,
                127.70559854247949,
            ],
        ),
        (
            8.0,
            [
                100.0,
                28.00041450333543,
                26.378803084925767,
                28.4339161265367,
                22.752796237247246,
                100.0,
                119.29332236764716,
                144.49285526648308,
                107.6294091848049,
                51.95116780562126,
                100.0,
                126.107229774437,
                294.7096444500288,
                256.4689457575154,
                286.139272112366,
            ],
        ),
    ];
    for (jump_intensity, expected) in cases {
        let i = ItofinMertonInput {
            jump_intensity,
            ..input()
        };
        let mut out = [0.0; 15];
        let mut e = error();
        assert_eq!(simulate(&i, &mut out, &mut e), 0);
        for (actual, expected) in out.into_iter().zip(expected) {
            assert!((actual - expected).abs() <= 2e-14 * expected.abs().max(1.0));
        }
    }
}
