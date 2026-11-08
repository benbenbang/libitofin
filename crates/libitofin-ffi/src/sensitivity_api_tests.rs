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
