use crate::boundary::*;
use crate::sensitivity_api::*;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::shared;
use std::ptr::{null, null_mut};

fn parallel(c: &mut Context, quote: u64, instrument: u64, kind: i32) -> (i32, String) {
    let mut out = ItofinSensitivity::default();
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let code = unsafe {
        itofin_sensitivity_parallel(
            c,
            &quote,
            1,
            &instrument,
            1,
            null(),
            0,
            1e-4,
            kind,
            null(),
            &mut out,
            &mut error,
        )
    };
    let message = unsafe { std::ffi::CStr::from_ptr(error.message.as_ptr()) };
    (code, message.to_string_lossy().into_owned())
}

#[test]
fn unsupported_instruments_and_kinds_are_errors_that_leave_the_quote() {
    let mut c = Context::new();
    let value = shared(SimpleQuote::new(0.03));
    let quote = c.insert(value.clone()).unwrap();
    let (code, message) = parallel(&mut c, quote, quote, 1);
    assert_eq!(code, INVALID_ARGUMENT);
    assert!(message.contains("credit default swaps"), "{message}");
    let (code, message) = parallel(&mut c, quote, u64::MAX, 1);
    assert_eq!(code, INVALID_ARGUMENT);
    assert!(message.contains("supports bonds"), "{message}");
    let (code, message) = parallel(&mut c, quote, quote, 2);
    assert_eq!(code, INVALID_ARGUMENT);
    assert!(message.contains("kind"), "{message}");
    let mut out = [ItofinSensitivity::default()];
    let code = unsafe {
        itofin_sensitivity_bucket(
            &mut c,
            &quote,
            1,
            &quote,
            1,
            null(),
            0,
            1e-4,
            1,
            out.as_mut_ptr(),
            null_mut(),
        )
    };
    assert_eq!(code, INVALID_ARGUMENT);
    assert_eq!(value.value().unwrap(), 0.03);
    let code = unsafe {
        itofin_sensitivity_parallel(
            &mut c,
            &quote,
            1,
            &quote,
            1,
            null(),
            0,
            1e-4,
            1,
            null(),
            null_mut(),
            null_mut(),
        )
    };
    assert_eq!(code, INVALID_ARGUMENT);
}

#[test]
fn black_scholes_from_quote_rejects_a_wrong_type_spot_handle() {
    use crate::market_api::itofin_black_scholes_from_quote;
    use libitofin::handle::Handle;
    use libitofin::interestrate::Compounding;
    use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    use libitofin::time::date::{Date, Month};
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use libitofin::time::frequency::Frequency;
    let mut c = Context::new();
    let today = Date::new(15, Month::June, 2026);
    let curve = Handle::new(shared(FlatForward::with_rate(
        today,
        0.05,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as libitofin::shared::Shared<dyn YieldTermStructure>);
    let vol = Handle::new(shared(BlackConstantVol::new(
        today,
        None,
        0.2,
        Actual365Fixed::new(),
    ))
        as libitofin::shared::Shared<dyn BlackVolTermStructure>);
    let curve = c.insert(curve).unwrap();
    let vol = c.insert(vol).unwrap();
    let spot = c.insert(shared(SimpleQuote::new(100.0))).unwrap();
    let mut out = 0;
    let mut call = |spot| unsafe {
        itofin_black_scholes_from_quote(&mut c, spot, curve, curve, vol, &mut out, null_mut())
    };
    assert_eq!(call(curve), INVALID_HANDLE);
    assert_eq!(call(vol), INVALID_HANDLE);
    assert_eq!(call(spot), 0);
}

fn option_on(c: &mut Context, strike: f64) -> (u64, u64) {
    use libitofin::exercise::{EuropeanExercise, Exercise};
    use libitofin::handle::Handle;
    use libitofin::instrument::Instrument;
    use libitofin::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
    use libitofin::interestrate::Compounding;
    use libitofin::option::OptionType;
    use libitofin::pricingengine::PricingEngine;
    use libitofin::pricingengines::AnalyticEuropeanEngine;
    use libitofin::processes::GeneralizedBlackScholesProcess;
    use libitofin::settings::Settings;
    use libitofin::shared::{SharedMut, shared_mut};
    use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    use libitofin::time::date::{Date, Month};
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use libitofin::time::frequency::Frequency;
    let today = Date::new(15, Month::June, 2026);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let curve = |rate: f64| {
        Handle::new(shared(FlatForward::with_rate(
            today,
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
            as libitofin::shared::Shared<dyn YieldTermStructure>)
    };
    let vol = Handle::new(shared(BlackConstantVol::new(
        today,
        None,
        0.2,
        Actual365Fixed::new(),
    ))
        as libitofin::shared::Shared<dyn BlackVolTermStructure>);
    let spot = shared(SimpleQuote::new(100.0));
    let process = shared(GeneralizedBlackScholesProcess::new(
        Handle::new(spot.clone() as libitofin::shared::Shared<dyn Quote>),
        curve(0.02),
        curve(0.05),
        vol,
    ));
    let payoff = shared(PlainVanillaPayoff::new(OptionType::Call, strike))
        as libitofin::shared::Shared<dyn StrikedTypePayoff>;
    let exercise: libitofin::shared::Shared<dyn Exercise> =
        shared(EuropeanExercise::new(today + 360));
    let option = shared_mut(VanillaOption::new(payoff, exercise, settings));
    let engine: SharedMut<dyn PricingEngine> = shared_mut(AnalyticEuropeanEngine::new(process));
    option.borrow_mut().base_mut().set_pricing_engine(engine);
    (c.insert(spot).unwrap(), c.insert(option).unwrap())
}

fn matrix(
    c: &mut Context,
    quotes: &[u64],
    rows: *const usize,
    row_count: usize,
    instruments: &[u64],
    out: &mut [ItofinSensitivity],
) -> i32 {
    unsafe {
        itofin_sensitivity_bucket_matrix(
            c,
            quotes.as_ptr(),
            quotes.len(),
            rows,
            row_count,
            instruments.as_ptr(),
            instruments.len(),
            null(),
            0,
            1e-4,
            1,
            out.as_mut_ptr(),
            null_mut(),
        )
    }
}

#[test]
fn matrix_rejects_row_lengths_that_miss_the_quote_count() {
    let mut c = Context::new();
    let (spot, option) = option_on(&mut c, 105.0);
    let mut out = [ItofinSensitivity::default(); 2];
    let quotes = [spot, spot];
    for rows in [[1usize, 0], [2, 1], [usize::MAX, 1]] {
        let code = matrix(
            &mut c,
            &quotes,
            rows.as_ptr(),
            rows.len(),
            &[option],
            &mut out,
        );
        assert_eq!(code, INVALID_ARGUMENT, "{rows:?}");
    }
    let code = matrix(&mut c, &quotes, null(), 1, &[option], &mut out);
    assert_eq!(code, INVALID_ARGUMENT);
    assert_eq!(out, [ItofinSensitivity::default(); 2]);
}

#[test]
fn matrix_writes_rows_back_to_back_across_an_empty_row() {
    let mut c = Context::new();
    let (low, low_option) = option_on(&mut c, 95.0);
    let (high, high_option) = option_on(&mut c, 115.0);
    let quotes = [low, high];
    let instruments = [low_option, high_option];
    let mut flat = [ItofinSensitivity::default(); 2];
    let code = unsafe {
        itofin_sensitivity_bucket(
            &mut c,
            quotes.as_ptr(),
            2,
            instruments.as_ptr(),
            2,
            null(),
            0,
            1e-4,
            1,
            flat.as_mut_ptr(),
            null_mut(),
        )
    };
    assert_eq!(code, 0);
    let mut out = [ItofinSensitivity::default(); 2];
    let rows = [1usize, 0, 1];
    assert_eq!(
        matrix(&mut c, &quotes, rows.as_ptr(), 3, &instruments, &mut out),
        0
    );
    assert_eq!(out, flat);
    assert!(out[0].first > out[1].first && out[1].first > 0.0 && out[0].has_second == 1);
}
