//! Explicit-base CPI bonds through the shared bond valuation interface.
use crate::bond_api::{NativeBond, optional_date};
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::inflation_api::{cpi_interpolation, zero_index};
use crate::rates_api::{finite, settings};
use crate::time_api::{adjust, advance, convention, day_counter, schedule_calendar, time_unit};
use libitofin::instruments::CpiBond;
use libitofin::time::{period::Period, schedule::Schedule};

/// Modern CPI bond configuration. Principal pays the full CPI ratio.
#[repr(C)]
pub struct ItofinCpiBondConfig {
    pub settlement_days: u32,
    pub face_amount: f64,
    pub base_cpi: f64,
    pub observation_lag_length: i32,
    pub observation_lag_unit: i32,
    pub index: u64,
    pub observation_interpolation: i32,
    pub schedule: u64,
    pub day_counter: u64,
    pub settings: u64,
    pub payment_convention: i32,
    /// Zero selects the schedule calendar.
    pub payment_calendar: u64,
    /// Zero means no issue date.
    pub issue_date: i32,
    pub ex_coupon_calendar: u64,
    pub ex_coupon_length: i32,
    pub ex_coupon_unit: i32,
    pub ex_coupon_convention: i32,
    pub has_ex_coupon: u8,
    pub ex_coupon_end_of_month: u8,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_cpi_bond_new(
    ctx: *mut Context,
    a: ItofinCpiBondConfig,
    coupons: *const f64,
    count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            finite(a.face_amount)?;
            finite(a.base_cpi)?;
            if a.has_ex_coupon > 1 || a.ex_coupon_end_of_month > 1 {
                return Err(BindingError::invalid("invalid ex-coupon flag"));
            }
            let rates = input_slice(coupons, count)?.to_vec();
            let schedule = c.get::<Schedule>(a.schedule)?;
            let calendar = if a.payment_calendar == 0 {
                schedule_calendar(c, a.schedule)?
            } else {
                c.get::<NativeCalendar>(a.payment_calendar)?
            };
            let rule = convention(a.payment_convention)?;
            crate::cpi_cashflows_api::check_leg_dates(
                &schedule,
                &schedule_calendar(c, a.schedule)?,
                &calendar,
                rule,
            )?;
            let ex_rule = convention(a.ex_coupon_convention)?;
            let ex_calendar = if a.has_ex_coupon == 1 {
                c.get::<NativeCalendar>(a.ex_coupon_calendar)?
            } else {
                calendar.clone()
            };
            let ex_period = if a.has_ex_coupon == 1 {
                if a.ex_coupon_length < 0 {
                    return Err(BindingError::invalid(
                        "ex-coupon period must be nonnegative",
                    ));
                }
                Some(Period::new(
                    a.ex_coupon_length,
                    time_unit(a.ex_coupon_unit)?,
                ))
            } else {
                None
            };
            for &date in schedule.dates().iter().skip(1) {
                let payment = adjust(&calendar, date, rule)?;
                if let Some(period) = ex_period {
                    advance(
                        &ex_calendar,
                        payment,
                        -period.length(),
                        period.units(),
                        ex_rule,
                        a.ex_coupon_end_of_month == 1,
                    )?;
                }
            }
            let bond = CpiBond::new(
                a.settlement_days,
                a.face_amount,
                a.base_cpi,
                Period::new(a.observation_lag_length, time_unit(a.observation_lag_unit)?),
                zero_index(c, a.index)?,
                cpi_interpolation(a.observation_interpolation)?,
                schedule,
                rates,
                day_counter(c, a.day_counter)?,
                rule,
                optional_date(a.issue_date)?,
                Some(calendar.inner.clone()),
                ex_period,
                ex_calendar.inner,
                ex_rule,
                a.ex_coupon_end_of_month == 1,
                settings(c, a.settings)?,
            )?;
            let mut native = NativeBond::new_unpriced(bond.into_bond(), calendar, None);
            native.indexed = true;
            output(out, c.insert(native)?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inflation_api::ZeroIndex;
    use libitofin::handle::RelinkableHandle;
    use libitofin::indexes::inflation::UkRpi;
    use libitofin::settings::Settings;
    use libitofin::shared::shared;
    use libitofin::time::{
        calendars::NullCalendar,
        date::{Date, Month::*},
        daycounters::actual365fixed::Actual365Fixed,
    };
    use std::ptr::{null, null_mut};

    fn setup() -> (Context, ItofinCpiBondConfig) {
        let mut context = Context::new();
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(15, June, 2025));
        let index = shared(UkRpi::new(settings.clone()));
        let index = context
            .insert(ZeroIndex {
                inner: index,
                curve: RelinkableHandle::empty(),
            })
            .unwrap();
        let settings = context.insert(settings).unwrap();
        let calendar = NativeCalendar {
            inner: NullCalendar::new(),
            horizon: None,
            first_year: 1901,
        };
        let schedule = context
            .insert_with_metadata(
                Schedule::from_dates(vec![
                    Date::new(15, January, 2025),
                    Date::new(15, July, 2025),
                ]),
                calendar,
            )
            .unwrap();
        let day_counter = context.insert(Actual365Fixed::new()).unwrap();
        (
            context,
            ItofinCpiBondConfig {
                settlement_days: 2,
                face_amount: 1000.0,
                base_cpi: 100.0,
                observation_lag_length: 3,
                observation_lag_unit: 2,
                index,
                observation_interpolation: 0,
                schedule,
                day_counter,
                settings,
                payment_convention: 2,
                payment_calendar: 0,
                issue_date: 0,
                ex_coupon_calendar: 0,
                ex_coupon_length: 0,
                ex_coupon_unit: 0,
                ex_coupon_convention: 2,
                has_ex_coupon: 0,
                ex_coupon_end_of_month: 0,
            },
        )
    }
    #[test]
    fn cpi_bond_lazy_construction_and_invalid_inputs_preserve_outputs() {
        let (mut context, config) = setup();
        let mut output = 777;
        unsafe {
            assert_eq!(
                itofin_cpi_bond_new(
                    &mut context,
                    config,
                    [0.02].as_ptr(),
                    1,
                    &mut output,
                    null_mut()
                ),
                0
            );
        }
        let native = context.get::<NativeBond>(output).unwrap();
        assert!(native.indexed);
        assert_eq!(native.bond.borrow().cashflows().len(), 2);
        assert_eq!(native.bond.borrow().redemptions().len(), 1);
        assert!(native.bond.borrow().redemptions()[0].amount().is_err());
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let (mut context, mut config) = setup();
            config.base_cpi = value;
            let mut output = 777;
            unsafe {
                assert_ne!(
                    itofin_cpi_bond_new(
                        &mut context,
                        config,
                        [0.02].as_ptr(),
                        1,
                        &mut output,
                        null_mut()
                    ),
                    0
                );
            }
            assert_eq!(output, 777);
        }
        let (mut context, config) = setup();
        let mut output = 777;
        unsafe {
            assert_ne!(
                itofin_cpi_bond_new(&mut context, config, null(), 1, &mut output, null_mut()),
                0
            );
        }
        assert_eq!(output, 777);
    }
    #[test]
    fn cpi_bond_pointer_preflight_precedes_output_and_handle_creation() {
        let (mut context, config) = setup();
        let misaligned = std::ptr::without_provenance_mut::<ItofinError>(1);
        let mut output = 777;
        unsafe {
            assert_eq!(
                itofin_cpi_bond_new(
                    &mut context,
                    config,
                    [0.02].as_ptr(),
                    1,
                    &mut output,
                    misaligned
                ),
                INVALID_ARGUMENT
            );
        }
        assert_eq!(output, 777);
        let (mut context, config) = setup();
        unsafe {
            assert_eq!(
                itofin_cpi_bond_new(
                    &mut context,
                    config,
                    [0.02].as_ptr(),
                    1,
                    null_mut(),
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
        }
    }
    #[test]
    fn cpi_bond_bad_handles_enums_flags_and_oversized_lag_are_recoverable() {
        for case in 0..8 {
            let (mut context, mut config) = setup();
            match case {
                0 => config.index = config.day_counter,
                1 => config.schedule = u64::MAX,
                2 => config.observation_interpolation = 27,
                3 => config.payment_convention = 27,
                4 => config.has_ex_coupon = 2,
                5 => config.observation_lag_length = i32::MAX,
                6 => config.ex_coupon_end_of_month = 2,
                _ => config.issue_date = i32::MAX,
            }
            let mut output = 777;
            unsafe {
                assert_ne!(
                    itofin_cpi_bond_new(
                        &mut context,
                        config,
                        [0.02].as_ptr(),
                        1,
                        &mut output,
                        null_mut()
                    ),
                    0
                );
            }
            assert_eq!(output, 777);
            assert!(context.insert(123_u32).is_ok());
        }
    }
    #[test]
    fn cpi_bond_settlement_overflow_is_rejected_without_poisoning() {
        let (mut context, mut config) = setup();
        config.settlement_days = (Date::max_date() - Date::min_date()) as u32;
        let mut bond = 0;
        unsafe {
            assert_eq!(
                itofin_cpi_bond_new(
                    &mut context,
                    config,
                    [0.02].as_ptr(),
                    1,
                    &mut bond,
                    null_mut()
                ),
                0
            );
            let mut output = 777.0;
            assert_ne!(
                crate::bond_api::itofin_bond_real(
                    &mut context,
                    bond,
                    0,
                    0,
                    &mut output,
                    null_mut()
                ),
                0
            );
            assert_eq!(output, 777.0);
        }
        assert!(context.insert(123_u32).is_ok());
    }
    #[test]
    fn cpi_bond_asset_swap_rejects_before_missing_fixings() {
        let (mut context, config) = setup();
        let mut bond = 0;
        unsafe {
            assert_eq!(
                itofin_cpi_bond_new(
                    &mut context,
                    config,
                    [0.02].as_ptr(),
                    1,
                    &mut bond,
                    null_mut()
                ),
                0
            );
            let config = crate::asset_swap_api::ItofinAssetSwapConfig {
                bond,
                index: 0,
                floating_schedule: 0,
                floating_day_counter: 0,
                clean_price: 100.0,
                spread: 0.0,
                pay_bond_coupon: 1,
            };
            let mut output = 777;
            assert_eq!(
                crate::asset_swap_api::itofin_asset_swap_new(
                    &mut context,
                    &config,
                    &mut output,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(output, 777);
        }
    }
    #[test]
    fn cpi_bond_raw_coupon_analytics_reject_before_fixing_or_curve_reads() {
        let (mut context, config) = setup();
        let day_counter = config.day_counter;
        let mut bond = 0;
        let mut output = 777.0;
        unsafe {
            assert_eq!(
                itofin_cpi_bond_new(
                    &mut context,
                    config,
                    [0.02].as_ptr(),
                    1,
                    &mut bond,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                crate::bond_analytics_api::itofin_bond_bps_from_curve(
                    &mut context,
                    bond,
                    0,
                    0,
                    &mut output,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(output, 777.0);
            let config = crate::bond_analytics_api::ItofinBondYieldAnalyticsConfig {
                day_counter,
                compounding: 2,
                frequency: 0,
                settlement: 0,
            };
            assert_eq!(
                crate::bond_analytics_api::itofin_bond_bps_from_yield(
                    &mut context,
                    bond,
                    0.04,
                    config,
                    &mut output,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(output, 777.0);
            assert_eq!(
                crate::bond_analytics_api::itofin_bond_atm_rate_from_curve(
                    &mut context,
                    bond,
                    0,
                    0,
                    &mut output,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(output, 777.0);
            assert_eq!(
                crate::bond_analytics_api::itofin_bond_atm_rate_from_price(
                    &mut context,
                    bond,
                    0,
                    100.0,
                    0,
                    0,
                    &mut output,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(output, 777.0);
        }
    }
}
