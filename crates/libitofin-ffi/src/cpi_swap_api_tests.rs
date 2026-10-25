use super::*;
#[allow(dead_code)]
#[path = "../../libitofin/tests/fixtures/cpi_swap/support.rs"]
mod support;
use libitofin::shared::shared;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use std::ptr::{null, null_mut};

struct Fixture {
    context: Context,
    config: ItofinCpiSwapConfig,
    discount: u64,
    market: support::Market,
}
impl Fixture {
    fn new() -> Self {
        let market = support::Market::new();
        let mut context = Context::new();
        let calendar = NativeCalendar {
            inner: NullCalendar::new(),
            horizon: None,
            first_year: 1901,
        };
        let schedule = context
            .insert_with_metadata(support::schedule(), calendar)
            .unwrap();
        let float_index = context
            .insert(NativeIbor::builtin(market.floating.clone()))
            .unwrap();
        let fixed_index = context
            .insert(crate::inflation_api::ZeroIndex {
                inner: market.index.clone(),
                curve: libitofin::handle::RelinkableHandle::empty(),
            })
            .unwrap();
        let dc = context.insert(Actual365Fixed::new()).unwrap();
        let settings = context.insert(market.settings.clone()).unwrap();
        let discount = context.insert(market.discount.clone()).unwrap();
        let config = ItofinCpiSwapConfig {
            swap_type: 0,
            nominal: 100000.0,
            inflation_nominal: 100000.0,
            subtract_inflation_nominal: 1,
            spread: 0.001,
            float_schedule: schedule,
            float_index,
            float_day_counter: dc,
            float_convention: 2,
            fixed_rate: 0.02,
            base_cpi: 100.0,
            fixed_schedule: schedule,
            fixed_index,
            fixed_day_counter: dc,
            fixed_convention: 2,
            lag_length: 3,
            lag_unit: 2,
            interpolation: 0,
            settings,
        };
        Self {
            context,
            config,
            discount,
            market,
        }
    }
    fn build(&mut self) -> u64 {
        let mut id = 0;
        assert_eq!(
            unsafe { itofin_cpi_swap_new(&mut self.context, &self.config, &mut id, null_mut()) },
            0
        );
        id
    }
    fn attach(&mut self, id: u64) {
        assert_eq!(
            unsafe {
                itofin_cpi_swap_set_engine(
                    &mut self.context,
                    id,
                    self.discount,
                    self.config.settings,
                    null_mut(),
                )
            },
            0
        );
    }
    fn value(&mut self, id: u64, query: i32) -> f64 {
        let mut out = f64::NAN;
        assert_eq!(
            unsafe { itofin_cpi_swap_value(&mut self.context, id, query, &mut out, null_mut()) },
            0
        );
        out
    }
}
#[test]
fn cpi_swap_typed_owner_values_and_live_cache() {
    let mut f = Fixture::new();
    let id = f.build();
    f.attach(id);
    let value = f.value(id, 0);
    assert!((value - f.value(id, 1) - f.value(id, 2)).abs() < 2e-9);
    assert!(f.value(id, 3) < 0.0);
    let fair = f.value(id, 4);
    f.config.spread = fair;
    let par = f.build();
    f.attach(par);
    assert!(f.value(par, 0).abs() < 2e-9);
    assert_eq!(
        unsafe { itofin_cpi_swap_action(&mut f.context, id, 2, null_mut()) },
        0
    );
    f.market.quote.set_value(0.05);
    assert_eq!(f.value(id, 0), value);
    assert_eq!(
        unsafe { itofin_cpi_swap_action(&mut f.context, id, 1, null_mut()) },
        0
    );
    assert!((f.value(id, 0) - value).abs() > 1.0);
}
#[test]
fn cpi_swap_errors_leave_outputs_unchanged() {
    let mut f = Fixture::new();
    let mut out = 999;
    assert_ne!(
        unsafe { itofin_cpi_swap_new(&mut f.context, null(), &mut out, null_mut()) },
        0
    );
    assert_eq!(out, 999);
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        f.config.base_cpi = invalid;
        assert_ne!(
            unsafe { itofin_cpi_swap_new(&mut f.context, &f.config, &mut out, null_mut()) },
            0
        );
        assert_eq!(out, 999);
    }
    f.config.base_cpi = 100.0;
    f.config.subtract_inflation_nominal = 2;
    assert_ne!(
        unsafe { itofin_cpi_swap_new(&mut f.context, &f.config, &mut out, null_mut()) },
        0
    );
    assert_eq!(out, 999);
    f.config.subtract_inflation_nominal = 1;
    let id = f.build();
    let mut value = 42.0;
    assert_ne!(
        unsafe { itofin_cpi_swap_value(&mut f.context, id, 0, &mut value, null_mut()) },
        0
    );
    assert_eq!(value, 42.0);
    assert_ne!(
        unsafe { itofin_cpi_swap_value(&mut f.context, id, 99, &mut value, null_mut()) },
        0
    );
    assert_eq!(value, 42.0);
    assert_ne!(
        unsafe { itofin_cpi_swap_value(&mut f.context, id, 5, null_mut(), null_mut()) },
        0
    );
    let misaligned = std::ptr::dangling_mut::<u8>().cast::<ItofinError>();
    assert_ne!(
        unsafe { itofin_cpi_swap_new(&mut f.context, &f.config, &mut out, misaligned) },
        0
    );
    assert_eq!(out, 999);
    let foreign = f.context.insert(shared(1.0)).unwrap();
    assert_ne!(
        unsafe { itofin_cpi_swap_value(&mut f.context, foreign, 0, &mut value, null_mut()) },
        0
    );
    assert_eq!(value, 42.0);
}
