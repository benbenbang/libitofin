use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::indexes_api::NativeIbor;
use crate::joint_curves_api::*;
use libitofin::handle::Handle;
use libitofin::indexes::{Euribor, Index};
use libitofin::quotes::SimpleQuote;
use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::time::{
    date::{Date, Month},
    daycounters::actual360::Actual360,
};
use std::ptr::{null, null_mut};

#[test]
fn joint_native_invalid_inputs_do_not_poison_the_session() {
    let mut c = Context::new();
    let settings = shared(Settings::new());
    let reference = Date::new(23, Month::October, 2025);
    settings.set_evaluation_date(reference);
    let base = shared(Euribor::three_months(Handle::empty(), settings.clone()));
    let other = shared(Euribor::six_months(Handle::empty(), settings));
    let calendar = c
        .insert(NativeCalendar {
            inner: base.fixing_calendar(),
            horizon: None,
            first_year: 1901,
        })
        .unwrap();
    let base_index = c.insert(NativeIbor::builtin(base)).unwrap();
    let other_index = c.insert(NativeIbor::builtin(other)).unwrap();
    let quote = c.insert(shared(SimpleQuote::new(0.002))).unwrap();
    let dc = c.insert(Actual360::new()).unwrap();
    let mut discount = 0;
    let mut out = 0;
    unsafe {
        assert_eq!(
            itofin_flat_forward_from_quote(
                &mut c,
                reference.serial_number(),
                quote,
                dc,
                &mut discount,
                null_mut()
            ),
            0
        );
    }
    let mut cfg = ItofinBasisHelperConfig {
        quote,
        tenor_length: 2,
        tenor_unit: 3,
        settlement_days: 2,
        calendar,
        convention: 1,
        end_of_month: 0,
        base_index,
        other_index,
        discount_curve: discount,
        bootstrap_base_curve: 1,
    };
    unsafe {
        assert_eq!(
            itofin_basis_helper_new(&mut c, null(), &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_basis_helper_new(&mut c, &cfg, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        cfg.end_of_month = 2;
        assert_eq!(
            itofin_basis_helper_new(&mut c, &cfg, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        cfg.end_of_month = 0;
        cfg.bootstrap_base_curve = -1;
        assert_eq!(
            itofin_basis_helper_new(&mut c, &cfg, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        cfg.bootstrap_base_curve = 1;
        cfg.tenor_length = i32::MAX;
        assert_eq!(
            itofin_basis_helper_new(&mut c, &cfg, &mut out, null_mut()),
            CORE_ERROR
        );
        cfg.tenor_length = 2;
        cfg.quote = 0;
        assert_eq!(
            itofin_basis_helper_new(&mut c, &cfg, &mut out, null_mut()),
            INVALID_HANDLE
        );
        cfg.quote = quote;
        assert_eq!(
            itofin_basis_helper_new(&mut c, &cfg, &mut out, null_mut()),
            0
        );
        assert_eq!(
            itofin_joint_curves_new(
                &mut c,
                reference.serial_number(),
                null(),
                0,
                null(),
                0,
                null(),
                1,
                dc,
                1e-10,
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_joint_curves_new(
                &mut c,
                reference.serial_number(),
                null(),
                0,
                null(),
                0,
                null(),
                0,
                dc,
                1e-10,
                &mut out,
                null_mut()
            ),
            CORE_ERROR
        );
        assert_eq!(
            itofin_joint_curve(&mut c, 0, -1, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_ibor_add_fixing(
                &mut c,
                base_index,
                reference.serial_number(),
                f64::NAN,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_ibor_add_fixing(
                &mut c,
                base_index,
                reference.serial_number(),
                0.03,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_ibor_add_fixing(
                &mut c,
                base_index,
                reference.serial_number(),
                0.031,
                null_mut()
            ),
            CORE_ERROR
        );
        assert_eq!(
            itofin_basis_helper_new(&mut c, &cfg, &mut out, null_mut()),
            0
        );
        let mut swap = crate::helpers_api::ItofinSwapHelperConfig {
            quote,
            tenor_length: i32::MAX,
            tenor_unit: 3,
            calendar,
            frequency: 1,
            convention: 0,
            day_counter: dc,
            index: other_index,
        };
        assert_eq!(
            itofin_swap_helper_with_discount(&mut c, &swap, discount, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        swap.tenor_length = 2;
        assert_eq!(
            itofin_swap_helper_with_discount(&mut c, &swap, discount, &mut out, null_mut()),
            0
        );
    }
}
