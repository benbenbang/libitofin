use super::*;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::Date;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use std::ptr::null_mut;

fn flat(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        Date::from_serial(45_000),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
fn config(
    c: &mut Context,
    base: Handle<dyn YieldTermStructure>,
    quote: Handle<dyn YieldTermStructure>,
) -> ItofinFxForwardConfig {
    ItofinFxForwardConfig {
        base_currency: c.insert(Currency::eur()).unwrap(),
        quote_currency: c.insert(Currency::usd()).unwrap(),
        spot: 1.1,
        base_curve: c.insert(base).unwrap(),
        quote_curve: c.insert(quote).unwrap(),
        base_notional: 1e6,
        strike: 1.12,
        spot_settlement_date: 45_002,
        delivery_date: 45_367,
        position: 0,
    }
}
#[test]
fn fx_forward_retains_exact_relinkable_handles_and_expiry_bypasses_empty_links() {
    let mut c = Context::new();
    let base: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
    let quote: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
    let cfg = config(&mut c, base.handle(), quote.handle());
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_fx_forward_new(&mut c, cfg, &mut id, null_mut()) },
        0
    );
    let mut value = -123.;
    assert_ne!(
        unsafe { itofin_fx_forward_value(&mut c, id, 3, 0, &mut value, null_mut()) },
        0
    );
    assert_eq!(value, -123.);
    assert_eq!(
        unsafe { itofin_fx_forward_value(&mut c, id, 4, 45_368, &mut value, null_mut()) },
        0
    );
    assert_eq!(value, 0.);
    base.link_to(flat(0.03));
    quote.link_to(flat(0.05));
    assert_eq!(
        unsafe { itofin_fx_forward_value(&mut c, id, 3, 0, &mut value, null_mut()) },
        0
    );
    assert!((value - 1.1 * 0.02_f64.exp()).abs() < 3e-15);
    quote.link_to(flat(0.08));
    assert_eq!(
        unsafe { itofin_fx_forward_value(&mut c, id, 3, 0, &mut value, null_mut()) },
        0
    );
    assert!((value - 1.1 * 0.05_f64.exp()).abs() < 3e-15);
}
#[test]
fn fx_forward_wrong_thread_wrong_type_and_invalid_constructor_preserve_outputs() {
    let mut c = Context::new();
    let mut cfg = config(&mut c, Handle::new(flat(0.03)), Handle::new(flat(0.05)));
    let mut id = 123;
    cfg.position = 2;
    assert_eq!(
        unsafe { itofin_fx_forward_new(&mut c, cfg, &mut id, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 123);
    let cfg = config(&mut c, Handle::new(flat(0.03)), Handle::new(flat(0.05)));
    assert_eq!(
        unsafe { itofin_fx_forward_new(&mut c, cfg, &mut id, null_mut()) },
        0
    );
    let address = (&raw mut c) as usize;
    let (status, value) = std::thread::spawn(move || {
        let mut value = -123.;
        let status = unsafe {
            itofin_fx_forward_value(address as *mut Context, id, 3, 0, &mut value, null_mut())
        };
        (status, value)
    })
    .join()
    .unwrap();
    assert_eq!(status, WRONG_THREAD);
    assert_eq!(value, -123.);
    let currency = c.insert(Currency::eur()).unwrap();
    let mut value = -123.;
    assert_eq!(
        unsafe { itofin_fx_forward_value(&mut c, currency, 3, 0, &mut value, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(value, -123.);
}

#[test]
fn fx_forward_misaligned_error_preserves_outputs_without_leaking_constructor() {
    let mut c = Context::new();
    let base = flat(0.03);
    let quote = flat(0.05);
    let cfg = config(
        &mut c,
        Handle::new(base.clone()),
        Handle::new(quote.clone()),
    );
    let inputs = [
        cfg.base_currency,
        cfg.quote_currency,
        cfg.base_curve,
        cfg.quote_curve,
    ];
    let mut storage = [0_u32; 300];
    let error = unsafe {
        storage
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<ItofinError>()
    };
    let mut id = 123;
    assert_eq!(
        unsafe { itofin_fx_forward_new(&mut c, cfg, &mut id, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 123);
    for input in inputs {
        assert_eq!(
            unsafe { itofin_handle_release(&mut c, input, null_mut()) },
            0
        );
    }
    assert_eq!(Shared::strong_count(&base), 1);
    assert_eq!(Shared::strong_count(&quote), 1);
    let cfg = config(&mut c, Handle::new(base), Handle::new(quote));
    assert_eq!(
        unsafe { itofin_fx_forward_new(&mut c, cfg, &mut id, null_mut()) },
        0
    );
    let mut value = -123.;
    let mut integer = -123;
    let mut currency = 123;
    assert_eq!(
        unsafe { itofin_fx_forward_value(&mut c, id, 3, 0, &mut value, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_fx_forward_integer(&mut c, id, 0, 0, &mut integer, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_fx_forward_currency(&mut c, id, 0, &mut currency, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(value, -123.);
    assert_eq!(integer, -123);
    assert_eq!(currency, 123);
    assert!(storage.iter().all(|v| *v == 0));
}
