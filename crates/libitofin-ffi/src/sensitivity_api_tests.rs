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
    assert!(message.contains("deferred (#1322)"), "{message}");
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
