use itofin_ffi::boundary::{CORE_ERROR, INVALID_ARGUMENT, ItofinError};
use itofin_ffi::simulation_api::{ItofinHestonInput, itofin_heston_paths};

fn input() -> ItofinHestonInput {
    ItofinHestonInput {
        spot: 100.0,
        variance: 0.04,
        risk_free_rate: 0.05,
        dividend_yield: 0.02,
        kappa: 1.2,
        theta: 0.06,
        sigma: 0.3,
        rho: -0.5,
        horizon: 1.0,
        steps: 4,
        paths: 3,
        seed: 42,
        scheme: 0,
        terminal_only: 0,
    }
}

fn error() -> ItofinError {
    ItofinError {
        code: -1,
        message: [0; 1024],
    }
}

#[test]
fn qem_fixture_and_terminal_layout_match_full_output() {
    let mut request = input();
    let mut full = [0.0; 30];
    let mut err = error();
    let code = unsafe { itofin_heston_paths(&request, full.as_mut_ptr(), full.len(), &mut err) };
    assert_eq!(code, 0);
    assert_eq!(err.code, 0);
    assert_eq!(&full[..2], &[100.0, 0.04]);
    assert!((full[2] - 93.218_736_641_315_03).abs() < 1e-11);
    assert!((full[3] - 0.065_675_158_526_020_28).abs() < 1e-13);

    request.terminal_only = 1;
    let mut terminal = [0.0; 6];
    let code =
        unsafe { itofin_heston_paths(&request, terminal.as_mut_ptr(), terminal.len(), &mut err) };
    assert_eq!(code, 0);
    for (path, state) in terminal.chunks_exact(2).enumerate() {
        assert_eq!(state, &full[path * 10 + 8..path * 10 + 10]);
    }
}

#[test]
fn qe_high_psi_fixture_and_zero_horizon() {
    let mut request = ItofinHestonInput {
        variance: 0.01,
        kappa: 0.5,
        theta: 0.01,
        sigma: 0.2,
        steps: 1,
        paths: 1,
        scheme: 1,
        terminal_only: 1,
        ..input()
    };
    let mut result = [0.0; 2];
    let mut err = error();
    let code =
        unsafe { itofin_heston_paths(&request, result.as_mut_ptr(), result.len(), &mut err) };
    assert_eq!(code, 0);
    assert!((result[0] - 96.553_174_001_572_44).abs() < 1e-11);
    assert!((result[1] - 0.018_076_059_597_846_472).abs() < 1e-13);

    request.horizon = 0.0;
    let code =
        unsafe { itofin_heston_paths(&request, result.as_mut_ptr(), result.len(), &mut err) };
    assert_eq!(code, 0);
    assert_eq!(result, [request.spot, request.variance]);
}

#[test]
fn invalid_inputs_and_capacity_preserve_output_and_report_errors() {
    let mut result = [123.0; 30];
    let mut err = error();
    let request = input();
    let code =
        unsafe { itofin_heston_paths(&request, result.as_mut_ptr(), result.len() - 1, &mut err) };
    assert_eq!(code, INVALID_ARGUMENT);
    assert_eq!(err.code, code);
    assert_ne!(err.message[0], 0);
    assert_eq!(result, [123.0; 30]);

    for (request, expected) in [
        (
            ItofinHestonInput {
                scheme: 2,
                ..input()
            },
            INVALID_ARGUMENT,
        ),
        (
            ItofinHestonInput {
                terminal_only: -1,
                ..input()
            },
            INVALID_ARGUMENT,
        ),
        (
            ItofinHestonInput {
                spot: f64::NAN,
                ..input()
            },
            CORE_ERROR,
        ),
        (
            ItofinHestonInput {
                paths: usize::MAX,
                ..input()
            },
            CORE_ERROR,
        ),
        (ItofinHestonInput { seed: 0, ..input() }, CORE_ERROR),
    ] {
        let code =
            unsafe { itofin_heston_paths(&request, result.as_mut_ptr(), result.len(), &mut err) };
        assert_eq!(code, expected);
        assert_eq!(err.code, code);
        assert_ne!(err.message[0], 0);
        assert_eq!(result, [123.0; 30]);
    }

    let code = unsafe {
        itofin_heston_paths(
            std::ptr::null(),
            result.as_mut_ptr(),
            result.len(),
            &mut err,
        )
    };
    assert_eq!(code, INVALID_ARGUMENT);
    assert_eq!(result, [123.0; 30]);
    let code =
        unsafe { itofin_heston_paths(&request, std::ptr::null_mut(), result.len(), &mut err) };
    assert_eq!(code, INVALID_ARGUMENT);

    let code =
        unsafe { itofin_heston_paths(&request, result.as_mut_ptr(), result.len(), &mut err) };
    assert_eq!(code, 0);
    assert_eq!(err.code, 0);
    assert_eq!(err.message[0], 0);
}
