use super::tests::request;
use super::*;

#[test]
fn invalid_and_unrepresentable_requests_fail_without_allocation() {
    let cases = [
        GjrRequest {
            spot: 0.0,
            ..request()
        },
        GjrRequest {
            spot: Real::NAN,
            ..request()
        },
        GjrRequest {
            daily_variance: -1.0,
            ..request()
        },
        GjrRequest {
            omega: -1.0,
            ..request()
        },
        GjrRequest {
            alpha: -1.0,
            ..request()
        },
        GjrRequest {
            beta: -1.0,
            ..request()
        },
        GjrRequest {
            gamma: -1.0,
            ..request()
        },
        GjrRequest {
            lambda: Real::INFINITY,
            ..request()
        },
        GjrRequest {
            days_per_year: 0.0,
            ..request()
        },
        GjrRequest {
            risk_free_rate: Real::NAN,
            ..request()
        },
        GjrRequest {
            dividend_yield: Real::INFINITY,
            ..request()
        },
        GjrRequest {
            risk_free_rate: Real::MAX,
            dividend_yield: -Real::MAX,
            ..request()
        },
        GjrRequest {
            horizon: -1.0,
            ..request()
        },
        GjrRequest {
            horizon: Real::INFINITY,
            ..request()
        },
        GjrRequest {
            horizon: Real::from_bits(1),
            ..request()
        },
        GjrRequest {
            steps: 0,
            ..request()
        },
        GjrRequest {
            paths: 0,
            ..request()
        },
        GjrRequest {
            seed: 0,
            ..request()
        },
        GjrRequest {
            steps: usize::MAX,
            ..request()
        },
        GjrRequest {
            paths: usize::MAX,
            ..request()
        },
        GjrRequest {
            daily_variance: Real::MAX,
            ..request()
        },
        GjrRequest {
            lambda: Real::MAX,
            ..request()
        },
        GjrRequest {
            days_per_year: Real::MAX,
            ..request()
        },
        GjrRequest {
            risk_free_rate: Real::MAX,
            horizon: 4.0,
            ..request()
        },
        GjrRequest {
            risk_free_rate: -Real::MAX,
            horizon: 4.0,
            ..request()
        },
    ];
    for case in cases {
        assert!(gjr_paths(&case).is_err());
    }
}

#[test]
fn output_and_work_dimension_overflow_are_checked_separately() {
    assert!(
        output_len(&GjrRequest {
            steps: usize::MAX,
            paths: 1,
            ..request()
        })
        .is_err()
    );
    assert!(
        output_len(&GjrRequest {
            steps: 1,
            paths: usize::MAX / 2 + 1,
            terminal_only: true,
            ..request()
        })
        .is_err()
    );
    let work_overflow = GjrRequest {
        steps: usize::MAX / 2 + 1,
        paths: 1,
        terminal_only: true,
        ..request()
    };
    assert_eq!(output_len(&work_overflow).unwrap(), 2);
    assert!(
        gjr_paths(&work_overflow)
            .unwrap_err()
            .message()
            .contains("work dimensions")
    );
    let allocation_overflow = GjrRequest {
        steps: 1,
        paths: usize::MAX / 32,
        terminal_only: false,
        ..request()
    };
    assert!(output_len(&allocation_overflow).is_ok());
    assert!(
        gjr_paths(&allocation_overflow)
            .unwrap_err()
            .message()
            .contains("allocation failed")
    );
}
