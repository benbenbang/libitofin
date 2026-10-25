use super::*;
use crate::cashflows_api::{itofin_cashflow_amount, itofin_leg_item, itofin_leg_npv};
use crate::inflation_api::ZeroIndex;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::{Index, inflation::UkRpi};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Month::*, daycounters::actual365fixed::Actual365Fixed};
use std::ptr::{null_mut, without_provenance_mut};

fn serial(day: i32, month: libitofin::time::date::Month, year: i32) -> i32 {
    Date::new(day, month, year).serial_number()
}
pub(super) fn fixture() -> (Context, u64, u64) {
    let mut c = Context::new();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(15, June, 2025));
    let index = shared(UkRpi::new(settings.clone()));
    for (month, year, value) in [
        (October, 2023, 90.0),
        (November, 2023, 91.0),
        (April, 2024, 110.0),
        (May, 2024, 112.0),
    ] {
        index.add_fixing(Date::new(1, month, year), value).unwrap();
    }
    let settings = c.insert(settings).unwrap();
    let index = c
        .insert(ZeroIndex {
            inner: index,
            curve: RelinkableHandle::empty(),
        })
        .unwrap();
    (c, index, settings)
}
pub(super) fn explicit(index: u64) -> ItofinCpiCashFlowConfig {
    ItofinCpiCashFlowConfig {
        index,
        nominal: 1000.0,
        base_cpi: 100.0,
        observation_date: serial(15, July, 2024),
        lag_length: 3,
        lag_unit: 2,
        interpolation: 0,
        payment_date: serial(15, July, 2025),
        growth_only: 1,
    }
}
pub(super) fn live(index: u64) -> ItofinZeroInflationCashFlowConfig {
    ItofinZeroInflationCashFlowConfig {
        index,
        nominal: 1000.0,
        start_date: serial(15, January, 2024),
        end_date: serial(15, July, 2024),
        lag_length: 3,
        lag_unit: 2,
        interpolation: 0,
        payment_date: serial(15, July, 2025),
        growth_only: 1,
    }
}

#[test]
fn inflation_principal_values_conversions_and_retained_graph() {
    let (mut c, index, settings) = fixture();
    let mut a = 0;
    let mut b = 0;
    let mut generic = 0;
    let mut leg = 0;
    let mut item = 0;
    let mut value = 0.0;
    unsafe {
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, explicit(index), &mut a, null_mut()),
            0
        );
        assert_eq!(
            itofin_zero_inflation_cashflow_new(&mut c, live(index), &mut b, null_mut()),
            0
        );
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, a, 0, &mut value, null_mut()),
            0
        );
        assert!((value - 100.0).abs() < 1e-10);
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, b, 0, &mut value, null_mut()),
            0
        );
        assert!((value - 1000.0 * (110.0 / 90.0 - 1.0)).abs() < 1e-10);
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, b, 0, &mut generic, null_mut()),
            0
        );
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, b, 1, &mut leg, null_mut()),
            0
        );
        assert_eq!(itofin_handle_release(&mut c, b, null_mut()), 0);
        assert_eq!(itofin_handle_release(&mut c, index, null_mut()), 0);
        assert_eq!(
            itofin_cashflow_amount(&mut c, generic, &mut value, null_mut()),
            0
        );
        assert!((value - 1000.0 * (110.0 / 90.0 - 1.0)).abs() < 1e-10);
        assert_eq!(itofin_leg_item(&mut c, leg, 0, &mut item, null_mut()), 0);
        assert_eq!(
            itofin_cashflow_amount(&mut c, item, &mut value, null_mut()),
            0
        );
        let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::with_rate(
            Date::new(15, June, 2025),
            0.0,
            Actual365Fixed::new(),
            libitofin::interestrate::Compounding::Continuous,
            libitofin::time::frequency::Frequency::Annual,
        ));
        let discount = c.insert(Handle::new(curve)).unwrap();
        assert_eq!(
            itofin_leg_npv(
                &mut c,
                leg,
                discount,
                settings,
                0,
                0,
                0,
                &mut value,
                null_mut()
            ),
            0
        );
        let other = c.insert(shared(Settings::<Date>::new())).unwrap();
        let saved = value;
        assert_eq!(
            itofin_leg_npv(
                &mut c,
                leg,
                discount,
                other,
                0,
                0,
                0,
                &mut value,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(value, saved);
    }
}

#[test]
fn inflation_principal_boundary_preserves_outputs_and_context() {
    let (mut c, index, _) = fixture();
    let mut out = 777;
    unsafe {
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, explicit(index), null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_cpi_cashflow_new(
                &mut c,
                explicit(index),
                &mut out,
                without_provenance_mut::<ItofinError>(1)
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(out, 777);
        for base in [0.0, 1e-20, f64::NAN, f64::INFINITY] {
            let mut a = explicit(index);
            a.base_cpi = base;
            assert_ne!(itofin_cpi_cashflow_new(&mut c, a, &mut out, null_mut()), 0);
            assert_eq!(out, 777);
        }
        for (lag, unit) in [(-1, 2), (i32::MAX, 2), (i32::MIN, 2), (3, 99)] {
            let mut a = live(index);
            a.lag_length = lag;
            a.lag_unit = unit;
            assert_ne!(
                itofin_zero_inflation_cashflow_new(&mut c, a, &mut out, null_mut()),
                0
            );
            assert_eq!(out, 777);
        }
        let mut a = live(index);
        a.start_date = 0;
        assert_eq!(
            itofin_zero_inflation_cashflow_new(&mut c, a, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        let mut a = explicit(index);
        a.interpolation = 99;
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, a, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        let mut a = live(index);
        a.growth_only = 2;
        assert_eq!(
            itofin_zero_inflation_cashflow_new(&mut c, a, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, explicit(index), &mut out, null_mut()),
            0
        );
        let id = out;
        let mut value = -42.0;
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 99, &mut value, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(value, -42.0);
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, index, 0, &mut value, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, id, 2, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(out, id);
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 0, &mut value, null_mut()),
            0
        );
    }
}

#[test]
fn inflation_principal_lazy_missing_fixing_and_zero_base_are_checked_after_erasure() {
    let (mut c, index, _) = fixture();
    let inner = zero_index(&c, index).unwrap();
    let mut a = live(index);
    a.start_date = serial(15, December, 2024);
    let mut id = 0;
    let mut flow = 0;
    let mut value = 23.0;
    unsafe {
        assert_eq!(
            itofin_zero_inflation_cashflow_new(&mut c, a, &mut id, null_mut()),
            0
        );
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, 23.0);
        inner
            .add_fixing(Date::new(1, September, 2024), 0.0)
            .unwrap();
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, id, 0, &mut flow, null_mut()),
            0
        );
        assert_eq!(
            itofin_cashflow_amount(&mut c, flow, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, 23.0);
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 1, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 0.0);
    }
}

#[test]
fn inflation_principal_query_date_guards_do_not_poison_context() {
    let (mut c, index, settings_id) = fixture();
    let settings = c.get::<Shared<Settings<Date>>>(settings_id).unwrap();
    let mut a = live(index);
    a.end_date = serial(15, December, 2199);
    a.interpolation = 1;
    a.payment_date = serial(31, December, 2199);
    let mut id = 0;
    let mut generic = 0;
    let mut value = -9.0;
    unsafe {
        assert_eq!(
            itofin_zero_inflation_cashflow_new(&mut c, a, &mut id, null_mut()),
            0
        );
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, id, 0, &mut generic, null_mut()),
            0
        );
        assert_eq!(
            itofin_cashflow_amount(&mut c, generic, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, -9.0);
        let mut e = explicit(index);
        e.observation_date = serial(15, December, 2199);
        e.interpolation = 1;
        e.payment_date = serial(31, December, 2199);
        assert_eq!(itofin_cpi_cashflow_new(&mut c, e, &mut id, null_mut()), 0);
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, id, 0, &mut generic, null_mut()),
            0
        );
        assert_eq!(
            itofin_cashflow_amount(&mut c, generic, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, -9.0);
        assert_eq!(
            itofin_cpi_cashflow_new(&mut c, explicit(index), &mut id, null_mut()),
            0
        );
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, id, 0, &mut generic, null_mut()),
            0
        );
        settings.set_evaluation_date(Date::new(1, January, 1901));
        assert_eq!(
            itofin_cashflow_amount(&mut c, generic, &mut value, null_mut()),
            CORE_ERROR
        );
        settings.set_evaluation_date(Date::new(15, June, 2025));
        assert_eq!(
            itofin_cashflow_amount(&mut c, generic, &mut value, null_mut()),
            0
        );
        assert!((value - 100.0).abs() < 1e-10);
    }
}

#[test]
fn inflation_principal_overflow_base_cannot_be_hidden_by_finite_ratio() {
    use libitofin::math::interpolations::linear::Linear;
    use libitofin::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
    use libitofin::termstructures::inflation::interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve;
    use libitofin::time::frequency::Frequency;
    let (mut c, index, _) = fixture();
    let original = zero_index(&c, index).unwrap();
    original
        .add_fixing(Date::new(1, January, 2025), 230.0)
        .unwrap();
    let curve: Shared<dyn ZeroInflationTermStructure> = shared(
        InterpolatedZeroInflationCurve::new(
            Date::new(15, June, 2025),
            vec![Date::new(1, January, 2025), Date::new(1, January, 2035)],
            vec![1e308, 1e308],
            Frequency::Monthly,
            Actual365Fixed::new(),
            Linear,
            None,
        )
        .unwrap(),
    );
    let linked = shared(original.clone_linked_to(Handle::new(curve)));
    let index = c
        .insert(ZeroIndex {
            inner: linked,
            curve: RelinkableHandle::empty(),
        })
        .unwrap();
    let mut a = live(index);
    a.start_date = serial(15, April, 2027);
    let mut id = 0;
    let mut generic = 0;
    let mut value = 31.0;
    unsafe {
        assert_eq!(
            itofin_zero_inflation_cashflow_new(&mut c, a, &mut id, null_mut()),
            0
        );
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 1, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, 31.0);
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(
            itofin_inflation_cashflow_convert(&mut c, id, 0, &mut generic, null_mut()),
            0
        );
        assert_eq!(
            itofin_cashflow_amount(&mut c, generic, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, 31.0);
        assert_eq!(
            itofin_inflation_cashflow_value(&mut c, id, 2, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 110.0);
    }
}
