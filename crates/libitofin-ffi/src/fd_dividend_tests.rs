//! Cash dividends through the C ABI (#1370): the FD Spot engine and the
//! analytic dividend European engine.
//!
//! REFERENCE SUBSTITUTION (#1369): the mid-life Spot values are cached from a
//! locally built C++ QuantLib 9863b578a running the `americanoption.cpp:731-761`
//! fixture with `FdBlackScholesVanillaEngine(process, dividends, 100, 400)`.
//! The C process uses a fixed reference date, which matches the C++
//! `FlatForward(0, NullCalendar())` because the evaluation date never moves.
use crate::boundary::*;
use crate::fd_engine_api::*;
use crate::market_api::itofin_black_scholes_new;
use crate::options_api::*;
use crate::time_api::itofin_day_counter_new;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::date::{Date, Month};
use std::ptr::{null, null_mut};

const GRID: ItofinFdConfig = ItofinFdConfig {
    present: 3,
    t_grid: 100,
    x_grid: 400,
    damping_steps: 0,
    scheme: 0,
};

struct Market {
    c: Context,
    settings: u64,
    process: u64,
    today: Date,
}

impl Market {
    fn new(today: Date, spot: f64, r: f64, q: f64, vol: f64) -> Market {
        let mut c = Context::new();
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settings = c.insert(Shared::clone(&settings)).unwrap();
        let (mut dc, mut process) = (0, 0);
        unsafe {
            assert_eq!(itofin_day_counter_new(&mut c, 0, &mut dc, null_mut()), 0);
            assert_eq!(
                itofin_black_scholes_new(
                    &mut c,
                    spot,
                    r,
                    q,
                    vol,
                    today.serial_number(),
                    dc,
                    &mut process,
                    null_mut()
                ),
                0
            );
        }
        Market {
            c,
            settings,
            process,
            today,
        }
    }

    fn fd(&mut self, cfg: ItofinFdConfig, dates: &[Date], amounts: &[f64]) -> u64 {
        let serials: Vec<i32> = dates.iter().map(|d| d.serial_number()).collect();
        let mut out = 0;
        let code = unsafe {
            itofin_fd_black_scholes_engine_with_cash_dividends(
                &mut self.c,
                self.process,
                cfg,
                serials.as_ptr(),
                amounts.as_ptr(),
                amounts.len(),
                &mut out,
                null_mut(),
            )
        };
        assert_eq!(code, 0);
        out
    }

    fn analytic(&mut self, dates: &[Date], amounts: &[f64]) -> u64 {
        let serials: Vec<i32> = dates.iter().map(|d| d.serial_number()).collect();
        let mut out = 0;
        let code = unsafe {
            itofin_analytic_dividend_european_engine_new(
                &mut self.c,
                self.process,
                serials.as_ptr(),
                amounts.as_ptr(),
                amounts.len(),
                &mut out,
                null_mut(),
            )
        };
        assert_eq!(code, 0);
        out
    }

    fn option(&mut self, kind: i32, strike: f64, expiry: Date, american: bool) -> u64 {
        let mut out = 0;
        let code = unsafe {
            itofin_option_new(
                &mut self.c,
                kind,
                strike,
                self.today.serial_number(),
                expiry.serial_number(),
                i32::from(american),
                self.settings,
                &mut out,
                null_mut(),
            )
        };
        assert_eq!(code, 0);
        out
    }

    fn value(&mut self, option: u64, engine: u64, field: i32) -> Result<f64, i32> {
        let mut value = -123.0;
        unsafe {
            assert_eq!(
                itofin_option_set_engine(&mut self.c, option, engine, 2, 0, null_mut()),
                0
            );
            match itofin_option_value(&mut self.c, option, field, &mut value, null_mut()) {
                0 => Ok(value),
                code => {
                    assert_eq!(value, -123.0);
                    Err(code)
                }
            }
        }
    }
}

fn cached_market() -> Market {
    Market::new(Date::new(27, Month::February, 2021), 100.0, 0.04, 0.08, 0.3)
}

fn maturity() -> Date {
    Date::new(27, Month::February, 2022)
}

/// `(kind, american, npv, delta, gamma)` as the C++ program printed them.
type Row = (i32, bool, [f64; 3]);

#[allow(
    clippy::excessive_precision,
    reason = "Preserve the exact digits QuantLib printed"
)]
const SINGLE: [Row; 2] = [
    (
        0,
        true,
        [
            9.3876035416433616,
            0.50439288657051151,
            0.014825610960493722,
        ],
    ),
    (
        1,
        false,
        [
            19.549295792091659,
            -0.57167700814074907,
            0.011801370341950067,
        ],
    ),
];

#[allow(
    clippy::excessive_precision,
    reason = "Preserve the exact digits QuantLib printed"
)]
const TWO: [Row; 2] = [
    (
        0,
        true,
        [7.9057708867966641, 0.49147953278341305, 0.01871370359184878],
    ),
    (
        1,
        false,
        [
            22.763590143100142,
            -0.62808693189317422,
            0.011427611391172732,
        ],
    ),
];

fn check_cached(m: &mut Market, dates: &[Date], amounts: &[f64], rows: &[Row]) -> Vec<f64> {
    let engine = m.fd(GRID, dates, amounts);
    let mut values = Vec::new();
    for &(kind, american, expected) in rows {
        let option = m.option(kind, 100.0, maturity(), american);
        for (field, want) in (0..).zip(expected) {
            let got = m.value(option, engine, field).unwrap();
            assert!(
                (got - want).abs() <= 1e-8,
                "{kind} {american} {field}: {got} vs {want}"
            );
            values.push(got);
        }
    }
    values
}

#[test]
fn spot_cash_dividends_reproduce_the_cached_quantlib_values() {
    let mut m = cached_market();
    let first = Date::new(27, Month::December, 2021);
    let single = check_cached(&mut m, &[first], &[10.0], &SINGLE);
    check_cached(
        &mut m,
        &[Date::new(27, Month::June, 2021), first],
        &[5.0, 10.0],
        &TWO,
    );
    let after = check_cached(
        &mut m,
        &[first, Date::new(27, Month::April, 2022)],
        &[10.0, 7.0],
        &SINGLE,
    );
    assert_eq!(single, after);
}

/// `testEuropeanKnownValue` (`dividendoption.cpp:166-221`): Hull 12.8.
#[test]
fn analytic_dividend_engine_reproduces_the_known_value() {
    let today = Date::new(27, Month::February, 2021);
    let mut m = Market::new(today, 40.0, 0.09, 0.0, 0.30);
    let engine = m.analytic(&[today + 60, today + 150], &[0.5, 0.5]);
    let option = m.option(0, 40.0, today + 180, false);
    let calculated = m.value(option, engine, 0).unwrap();
    assert!((calculated - 3.67).abs() <= 1e-2, "{calculated}");
    let american = m.option(0, 40.0, today + 180, true);
    assert_eq!(m.value(american, engine, 0), Err(CORE_ERROR));
}

/// `testFdDividendAtTZero` (`dividendoption.cpp:856-919`), Spot arm, and the
/// theta the C++ refuses (`:889-892`) as a core error.
#[test]
fn dividend_today_matches_the_analytic_engine_and_has_no_theta() {
    let today = Date::new(27, Month::February, 2005);
    let expiry = Date::new(13, Month::April, 2005);
    let mut m = Market::new(today, 54.625, 0.0, 0.0, 0.282922);
    let cfg = ItofinFdConfig { t_grid: 50, ..GRID };
    let fd = m.fd(cfg, &[today], &[1.0]);
    let analytic = m.analytic(&[today], &[1.0]);
    let european = m.option(0, 55.0, expiry, false);
    let expected = m.value(european, analytic, 0).unwrap();
    for american in [false, true] {
        let option = m.option(0, 55.0, expiry, american);
        let calculated = m.value(option, fd, 0).unwrap();
        assert!(
            (calculated - expected).abs() <= 1e-4,
            "{calculated} {expected}"
        );
        assert_eq!(m.value(option, fd, 3), Err(CORE_ERROR));
    }
}

#[test]
fn invalid_dividend_inputs_preserve_the_output() {
    let mut m = cached_market();
    let dates = [maturity().serial_number()];
    let amounts = [1.0];
    let invalid = [0];
    let process = m.process;
    let c = &mut m.c;
    let mut out = 77;
    let cases: [(*const i32, *const f64, usize, u64, i32); 5] = [
        (null(), amounts.as_ptr(), 1, process, INVALID_ARGUMENT),
        (dates.as_ptr(), null(), 1, process, INVALID_ARGUMENT),
        (
            invalid.as_ptr(),
            amounts.as_ptr(),
            1,
            process,
            INVALID_ARGUMENT,
        ),
        (dates.as_ptr(), amounts.as_ptr(), 1, 0, INVALID_HANDLE),
        (null(), null(), 0, 0, INVALID_HANDLE),
    ];
    for (d, a, n, p, code) in cases {
        unsafe {
            assert_eq!(
                itofin_fd_black_scholes_engine_with_cash_dividends(
                    c,
                    p,
                    GRID,
                    d,
                    a,
                    n,
                    &mut out,
                    null_mut()
                ),
                code
            );
            assert_eq!(
                itofin_analytic_dividend_european_engine_new(c, p, d, a, n, &mut out, null_mut()),
                code
            );
        }
    }
    let bad = ItofinFdConfig {
        present: 16,
        ..GRID
    };
    unsafe {
        assert_eq!(
            itofin_fd_black_scholes_engine_with_cash_dividends(
                c,
                process,
                bad,
                dates.as_ptr(),
                amounts.as_ptr(),
                1,
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_analytic_dividend_european_engine_new(
                c,
                process,
                dates.as_ptr(),
                amounts.as_ptr(),
                1,
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(out, 77);
}

#[test]
fn engines_retain_the_process_and_an_empty_schedule_is_the_plain_engine() {
    let mut m = cached_market();
    let mut plain = 0;
    let mut empty = 0;
    let mut analytic = 0;
    unsafe {
        assert_eq!(
            itofin_fd_black_scholes_engine_new(&mut m.c, m.process, GRID, &mut plain, null_mut()),
            0
        );
        assert_eq!(
            itofin_fd_black_scholes_engine_with_cash_dividends(
                &mut m.c,
                m.process,
                GRID,
                null(),
                null(),
                0,
                &mut empty,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_analytic_dividend_european_engine_new(
                &mut m.c,
                m.process,
                null(),
                null(),
                0,
                &mut analytic,
                null_mut()
            ),
            0
        );
    }
    let dividend = m.fd(GRID, &[Date::new(27, Month::December, 2021)], &[10.0]);
    unsafe {
        assert_eq!(itofin_handle_release(&mut m.c, m.process, null_mut()), 0);
    }
    for american in [false, true] {
        let option = m.option(1, 100.0, maturity(), american);
        for field in 0..4 {
            assert_eq!(
                m.value(option, empty, field).unwrap().to_bits(),
                m.value(option, plain, field).unwrap().to_bits()
            );
        }
        let expected = SINGLE[1].2[0];
        if !american {
            assert!((m.value(option, dividend, 0).unwrap() - expected).abs() <= 1e-8);
            assert!(m.value(option, analytic, 0).unwrap().is_finite());
        }
    }
}
