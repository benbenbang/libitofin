use itofin_ffi::boundary::{CORE_ERROR, INVALID_ARGUMENT, ItofinError};
use itofin_ffi::gjr_api::{ItofinGjrInput, itofin_gjr_paths};
use libitofin::methods::montecarlo::gjr_paths::{GjrRequest, gjr_paths};
use libitofin::processes::GjrGarchDiscretization;
use std::ptr::{null, null_mut};
fn input() -> ItofinGjrInput {
    ItofinGjrInput {
        spot: 100.0,
        daily_variance: 0.04 / 252.0,
        risk_free_rate: 0.05,
        dividend_yield: 0.02,
        omega: 2e-6,
        alpha: 0.024,
        beta: 0.93,
        gamma: 0.059,
        lambda: 0.1,
        days_per_year: 252.0,
        horizon: 1.0,
        steps: 4,
        paths: 3,
        seed: 42,
        discretization: 1,
        terminal_only: 0,
    }
}
fn simulate(i: &ItofinGjrInput, out: &mut [f64]) -> i32 {
    unsafe { itofin_gjr_paths(i, out.as_mut_ptr(), out.len(), null_mut()) }
}
#[test]
fn schemes_replay_core_parity_and_terminal_layout() {
    for (flag, scheme) in [
        (0, GjrGarchDiscretization::PartialTruncation),
        (1, GjrGarchDiscretization::FullTruncation),
        (2, GjrGarchDiscretization::Reflection),
    ] {
        let mut i = ItofinGjrInput {
            discretization: flag,
            ..input()
        };
        let mut full = [0.0; 30];
        assert_eq!(simulate(&i, &mut full), 0);
        let r = GjrRequest {
            spot: i.spot,
            daily_variance: i.daily_variance,
            risk_free_rate: i.risk_free_rate,
            dividend_yield: i.dividend_yield,
            omega: i.omega,
            alpha: i.alpha,
            beta: i.beta,
            gamma: i.gamma,
            lambda: i.lambda,
            days_per_year: i.days_per_year,
            horizon: i.horizon,
            steps: i.steps,
            paths: i.paths,
            seed: i.seed,
            discretization: scheme,
            terminal_only: false,
        };
        assert_eq!(full.to_vec(), gjr_paths(&r).unwrap());
        let mut again = [0.0; 30];
        assert_eq!(simulate(&i, &mut again), 0);
        assert_eq!(full, again);
        for path in full.chunks_exact(10) {
            assert_eq!(&path[..2], &[100.0, 0.04]);
        }
        i.terminal_only = 1;
        let mut terminal = [0.0; 6];
        assert_eq!(simulate(&i, &mut terminal), 0);
        for (path, state) in terminal.chunks_exact(2).enumerate() {
            assert_eq!(state, &full[path * 10 + 8..path * 10 + 10]);
        }
        i.horizon = 0.0;
        assert_eq!(simulate(&i, &mut terminal), 0);
        assert_eq!(terminal, [100.0, 0.04, 100.0, 0.04, 100.0, 0.04]);
    }
}
#[test]
fn null_misaligned_short_flags_and_domains_preserve_output() {
    let i = input();
    let mut out = [91.0; 30];
    let mut e = ItofinError {
        code: -1,
        message: [0; 1024],
    };
    unsafe {
        assert_eq!(
            itofin_gjr_paths(null(), out.as_mut_ptr(), 30, &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_paths(&i, out.as_mut_ptr(), 29, &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_paths(&i, null_mut(), 30, &mut e),
            INVALID_ARGUMENT
        );
        let bad_input = (&raw const i).cast::<u8>().add(1).cast::<ItofinGjrInput>();
        let bad_output = out.as_mut_ptr().cast::<u8>().add(1).cast::<f64>();
        let bad_error = (&raw mut e).cast::<u8>().add(1).cast::<ItofinError>();
        assert_eq!(
            itofin_gjr_paths(bad_input, out.as_mut_ptr(), 30, &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_paths(&i, bad_output, 30, &mut e),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_paths(&i, out.as_mut_ptr(), 30, bad_error),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_paths(&i, out.as_mut_ptr(), usize::MAX, &mut e),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(out, [91.0; 30]);
    for (i, code) in [
        (
            ItofinGjrInput {
                discretization: 3,
                ..input()
            },
            INVALID_ARGUMENT,
        ),
        (
            ItofinGjrInput {
                terminal_only: -1,
                ..input()
            },
            INVALID_ARGUMENT,
        ),
        (
            ItofinGjrInput {
                daily_variance: -1.0,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinGjrInput {
                omega: -1.0,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinGjrInput {
                lambda: f64::NAN,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinGjrInput {
                days_per_year: 0.0,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinGjrInput {
                spot: 0.0,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinGjrInput {
                paths: usize::MAX,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinGjrInput {
                steps: usize::MAX,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinGjrInput {
                horizon: -1.0,
                ..input()
            },
            CORE_ERROR,
        ),
        (ItofinGjrInput { seed: 0, ..input() }, CORE_ERROR),
    ] {
        assert_eq!(simulate(&i, &mut out), code);
        assert_eq!(out, [91.0; 30]);
    }
    assert_eq!(simulate(&input(), &mut out), 0);
}
