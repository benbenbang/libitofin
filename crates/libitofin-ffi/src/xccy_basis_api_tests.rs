use super::*;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::{IborIndex, Index};
use libitofin::interestrate::Compounding;
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{
    calendars::NullCalendar, daycounters::actual360::Actual360, frequency::Frequency,
    period::Period, timeunit::TimeUnit,
};
use std::ptr::{null, null_mut};

fn flat(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        Date::from_serial(46027),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
fn fixture(
    c: &mut Context,
) -> (
    ItofinXccyBasisSwapConfig,
    [Shared<IborIndex>; 2],
    RelinkableHandle<dyn YieldTermStructure>,
) {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::from_serial(46027));
    let base = shared(IborIndex::new(
        "C-Xccy-Base".into(),
        Period::new(3, TimeUnit::Months),
        0,
        Currency::eur(),
        NullCalendar::new(),
        BusinessDayConvention::Unadjusted,
        false,
        Actual360::new(),
        Handle::new(flat(0.031)),
        settings.clone(),
    ));
    let quote = shared(IborIndex::new(
        "C-Xccy-Quote".into(),
        Period::new(3, TimeUnit::Months),
        0,
        Currency::usd(),
        NullCalendar::new(),
        BusinessDayConvention::Unadjusted,
        false,
        Actual360::new(),
        Handle::new(flat(0.043)),
        settings.clone(),
    ));
    let dates = [46030, 46120, 46211, 46303, 46395]
        .map(Date::from_serial)
        .to_vec();
    let schedule = Schedule::from_dates(dates);
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let base_schedule = c
        .insert_with_metadata(schedule.clone(), calendar.clone())
        .unwrap();
    let quote_schedule = c.insert_with_metadata(schedule, calendar).unwrap();
    let discount = RelinkableHandle::new(flat(0.036));
    let cfg = ItofinXccyBasisSwapConfig {
        base_schedule,
        quote_schedule,
        base_index: c.insert(NativeIbor::builtin(base.clone())).unwrap(),
        quote_index: c.insert(NativeIbor::builtin(quote.clone())).unwrap(),
        base_curve: c.insert(Handle::new(flat(0.027))).unwrap(),
        quote_curve: c.insert(discount.handle()).unwrap(),
        base_day_counter: c.insert(Actual360::new()).unwrap(),
        quote_day_counter: c.insert(Actual360::new()).unwrap(),
        base_currency: c.insert(Currency::eur()).unwrap(),
        quote_currency: c.insert(Currency::usd()).unwrap(),
        settings: c.insert(settings).unwrap(),
        base_notional: 1e6,
        quote_notional: 1_270_000.,
        quote_spread: 0.0017,
        spot: 1.27,
        has_payment_convention: 0,
        payment_convention: 0,
        has_valuation_dates: 0,
        settlement_date: 0,
        npv_date: 0,
        include_settlement_flows: 0,
        has_spot_settlement_date: 0,
        spot_settlement_date: 0,
    };
    (cfg, [base, quote], discount)
}
fn create(c: &mut Context, cfg: &ItofinXccyBasisSwapConfig) -> u64 {
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_new(c, cfg, &mut id, null_mut()) },
        0
    );
    id
}
fn value(c: &mut Context, id: u64, query: i32) -> f64 {
    let mut out = 987.;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_value(c, id, query, &mut out, null_mut()) },
        0
    );
    out
}
#[test]
fn xccy_c_native_oracle_relinks_retention_and_currency_copies() {
    let mut c = Context::new();
    let (cfg, _, discount) = fixture(&mut c);
    let id = create(&mut c, &cfg);
    let expected = [
        4015.3856141086876,
        11035.711320720658,
        125.82851287923141,
        4674.150858899704,
        5_936.171_590_802_625,
        -0.003_017_668_082_511_701,
        1.27,
    ];
    for (query, want) in expected.into_iter().enumerate() {
        let got = value(&mut c, id, query as i32);
        assert!(
            (got - want).abs() <= if query == 5 { 2e-13 } else { 2e-9 },
            "query{query}: {got} vs {want}"
        );
    }
    let before = value(&mut c, id, 4);
    discount.link_to(flat(0.075));
    assert_ne!(value(&mut c, id, 4), before);
    for handle in [
        cfg.base_index,
        cfg.quote_index,
        cfg.base_curve,
        cfg.quote_curve,
        cfg.base_schedule,
        cfg.quote_schedule,
        cfg.settings,
    ] {
        assert_eq!(
            unsafe { crate::boundary::itofin_handle_release(&mut c, handle, null_mut()) },
            0
        );
    }
    assert!(value(&mut c, id, 4).is_finite());
    let mut copy = 0;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_currency(&mut c, id, 0, &mut copy, null_mut()) },
        0
    );
    assert_eq!(
        unsafe { crate::boundary::itofin_handle_release(&mut c, id, null_mut()) },
        0
    );
    assert_eq!(c.get::<Currency>(copy).unwrap(), Currency::eur());
}
#[test]
fn xccy_c_invalid_flags_pointers_and_foreign_handles_preserve_outputs() {
    let mut c = Context::new();
    let (mut cfg, _, _) = fixture(&mut c);
    let mut out = 987;
    cfg.has_valuation_dates = 2;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_new(&mut c, &cfg, &mut out, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 987);
    cfg.has_valuation_dates = 0;
    cfg.include_settlement_flows = 1;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_new(&mut c, &cfg, &mut out, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 987);
    cfg.include_settlement_flows = 0;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_new(&mut c, null(), &mut out, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 987);
    cfg.quote_spread = f64::NAN;
    assert_ne!(
        unsafe { itofin_xccy_basis_swap_new(&mut c, &cfg, &mut out, null_mut()) },
        0
    );
    assert_eq!(out, 987);
    cfg.quote_spread = 0.0017;
    let mut foreign = Context::new();
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_new(&mut foreign, &cfg, &mut out, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(out, 987);
    let id = create(&mut c, &cfg);
    let mut result = 987.;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_value(&mut c, id, 7, &mut result, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(result, 987.);
    assert_eq!(
        unsafe {
            itofin_xccy_basis_swap_value(&mut c, cfg.base_currency, 0, &mut result, null_mut())
        },
        INVALID_HANDLE
    );
    assert_eq!(result, 987.);
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_currency(&mut c, id, 2, &mut out, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 987);
    let address = (&raw mut c) as usize;
    let status = std::thread::spawn(move || {
        let mut out = 987.;
        let status = unsafe {
            itofin_xccy_basis_swap_value(address as *mut Context, id, 0, &mut out, null_mut())
        };
        assert_eq!(out, 987.);
        status
    })
    .join()
    .unwrap();
    assert_eq!(status, WRONG_THREAD);
    let mut bytes = [0u64; 140];
    let misaligned = unsafe { bytes.as_mut_ptr().cast::<u8>().add(1).cast::<ItofinError>() };
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_new(&mut c, &cfg, &mut out, misaligned) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 987);
}
#[test]
fn xccy_c_leg_only_queries_do_not_demand_the_other_fixings() {
    let mut c = Context::new();
    let (cfg, indexes, _) = fixture(&mut c);
    let id = create(&mut c, &cfg);
    indexes[0]
        .add_fixing(Date::from_serial(46030), 0.022)
        .unwrap();
    c.get::<Shared<Settings<Date>>>(cfg.settings)
        .unwrap()
        .set_evaluation_date(Date::from_serial(46031));
    assert!(value(&mut c, id, 0).is_finite());
    let mut out = 987.;
    for query in 1..=5 {
        assert_ne!(
            unsafe { itofin_xccy_basis_swap_value(&mut c, id, query, &mut out, null_mut()) },
            0
        );
        assert_eq!(out, 987.);
    }
    assert_eq!(value(&mut c, id, 6), 1.27);
    indexes[1]
        .add_fixing(Date::from_serial(46030), 0.034)
        .unwrap();
    assert!(value(&mut c, id, 4).is_finite());
}

#[test]
fn xccy_c_initial_empty_discount_links_recover_without_reconstruction() {
    let mut c = Context::new();
    let (mut cfg, _, _) = fixture(&mut c);
    let base: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
    let quote: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
    cfg.base_curve = c.insert(base.handle()).unwrap();
    cfg.quote_curve = c.insert(quote.handle()).unwrap();
    let id = create(&mut c, &cfg);
    let mut out = 987.;
    assert_ne!(
        unsafe { itofin_xccy_basis_swap_value(&mut c, id, 4, &mut out, null_mut()) },
        0
    );
    assert_eq!(out, 987.);
    assert_eq!(value(&mut c, id, 6), 1.27);
    base.link_to(flat(0.027));
    quote.link_to(flat(0.036));
    assert!((value(&mut c, id, 4) - 5_936.171_590_802_625).abs() < 2e-9);
    quote.link_to(flat(0.075));
    assert_ne!(value(&mut c, id, 4), 5_936.171_590_802_625);
}

#[test]
fn xccy_c_retained_calendar_guards_are_leg_selective_and_do_not_poison() {
    let mut c = Context::new();
    let (mut cfg, _, _) = fixture(&mut c);
    let mut index = c.get::<NativeIbor>(cfg.quote_index).unwrap();
    index.inner = shared(IborIndex::new(
        "Guarded-C-Xccy-Quote".into(),
        Period::new(3, TimeUnit::Months),
        0,
        Currency::usd(),
        NullCalendar::new(),
        BusinessDayConvention::Following,
        false,
        Actual360::new(),
        index.inner.forwarding_term_structure().clone(),
        c.get::<Shared<Settings<Date>>>(cfg.settings).unwrap(),
    ));
    index.maturity_calendar.horizon = Some(2026);
    cfg.quote_index = c.insert(index).unwrap();
    let id = create(&mut c, &cfg);
    assert!(value(&mut c, id, 0).is_finite());
    let mut out = 987.;
    assert_ne!(
        unsafe { itofin_xccy_basis_swap_value(&mut c, id, 1, &mut out, null_mut()) },
        0
    );
    assert_eq!(out, 987.);
    assert_eq!(value(&mut c, id, 6), 1.27);
    let mut currency = 0;
    assert_eq!(
        unsafe { itofin_xccy_basis_swap_currency(&mut c, id, 0, &mut currency, null_mut()) },
        0
    );
    assert_eq!(c.get::<Currency>(currency).unwrap(), Currency::eur());
}
