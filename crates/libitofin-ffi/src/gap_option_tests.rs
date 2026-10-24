use crate::boundary::*;
use crate::options_api::itofin_option_gap_new;
use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::time::date::{Date, Month};
use std::ptr::null_mut;

/// The context stays live and exclusively accessed; the joined foreign thread
/// only exercises owner rejection before any graph access, then the owner resumes.
#[test]
fn gap_factory_rejects_wrong_thread_without_mutating_output() {
    let mut context = Context::new();
    let settings = context.insert(shared(Settings::<Date>::new())).unwrap();
    let address = std::ptr::addr_of_mut!(context) as usize;
    let expiry = Date::new(15, Month::June, 2027).serial_number();
    std::thread::spawn(move || {
        let mut out = 123;
        let status = unsafe {
            itofin_option_gap_new(
                address as *mut Context,
                0,
                100.0,
                120.0,
                expiry,
                settings,
                &mut out,
                null_mut(),
            )
        };
        assert_eq!(status, WRONG_THREAD);
        assert_eq!(out, 123);
    })
    .join()
    .unwrap();
    let mut out = 0;
    let status = unsafe {
        itofin_option_gap_new(
            &mut context,
            0,
            100.0,
            120.0,
            expiry,
            settings,
            &mut out,
            null_mut(),
        )
    };
    assert_eq!(status, 0);
    assert_ne!(out, 0);
}
