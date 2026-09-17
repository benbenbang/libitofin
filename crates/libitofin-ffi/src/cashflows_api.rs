//! Generic cash flows and immutable Ibor leg configurations.
use crate::boundary::*;
use crate::rates_api::{curve, finite, settings};
use crate::time_api::{convention, date, day_counter};
use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::{CashFlows, IborCoupon, IborLeg};
use libitofin::indexes::IborIndex;
use libitofin::shared::Shared;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, daycounter::DayCounter, schedule::Schedule,
};
use libitofin::types::Real;

#[derive(Clone)]
pub(crate) struct IborLegConfig {
    schedule: Schedule,
    index: Shared<IborIndex>,
    notional: Option<Real>,
    day_counter: Option<DayCounter>,
    adjustment: Option<BusinessDayConvention>,
    fixing_days: Option<u32>,
}
impl IborLegConfig {
    fn builder(&self) -> IborLeg {
        let mut b = IborLeg::new(self.schedule.clone(), self.index.clone());
        if let Some(v) = self.notional {
            b = b.with_notional(v);
        }
        if let Some(v) = &self.day_counter {
            b = b.with_payment_day_counter(v.clone());
        }
        if let Some(v) = self.adjustment {
            b = b.with_payment_adjustment(v);
        }
        if let Some(v) = self.fixing_days {
            b = b.with_fixing_days(v);
        }
        b
    }
    pub(crate) fn coupons(&self) -> BindingResult<Vec<Shared<IborCoupon>>> {
        Ok(self.builder().coupons()?)
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_leg_new(
    ctx: *mut Context,
    schedule: u64,
    index: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let b = IborLegConfig {
                schedule: c.get(schedule)?,
                index: crate::indexes_api::ibor_index(c, index)?,
                notional: None,
                day_counter: None,
                adjustment: None,
                fixing_days: None,
            };
            output(out, c.insert(b)?)
        })
    }
}
/// Copy with one override: 0 notional, 1 day counter handle, 2 adjustment, 3 fixing days.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_leg_with(
    ctx: *mut Context,
    id: u64,
    field: i32,
    real: Real,
    integer: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let mut b = c.get::<IborLegConfig>(id)?;
            match field {
                0 => b.notional = Some(finite(real)?),
                1 => b.day_counter = Some(day_counter(c, integer)?),
                2 => {
                    b.adjustment =
                        Some(convention(i32::try_from(integer).map_err(|_| {
                            BindingError::invalid("convention overflow")
                        })?)?)
                }
                3 => {
                    b.fixing_days = Some(
                        u32::try_from(integer)
                            .map_err(|_| BindingError::invalid("fixing days overflow"))?,
                    )
                }
                _ => return Err(BindingError::invalid("invalid leg override")),
            }
            output(out, c.insert(b)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_leg_count(
    ctx: *mut Context,
    id: u64,
    out: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(out, c.get::<IborLegConfig>(id)?.coupons()?.len())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_ibor_leg_build(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let leg: Leg = c.get::<IborLegConfig>(id)?.builder().build()?;
            output(out, c.insert(leg)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_leg_count(
    ctx: *mut Context,
    id: u64,
    out: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe { with_context(ctx, error, |c| output(out, c.get::<Leg>(id)?.len())) }
}
/// Negative indices count backwards, matching the Python sequence interface.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_leg_item(
    ctx: *mut Context,
    id: u64,
    index: i64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let leg = c.get::<Leg>(id)?;
            let n = leg.len() as i64;
            let resolved = if index < 0 {
                index.checked_add(n)
            } else {
                Some(index)
            };
            let i = resolved
                .filter(|i| *i >= 0 && *i < n)
                .ok_or_else(|| BindingError::invalid("cashflow index out of range"))?;
            output(out, c.insert(leg[i as usize].clone())?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cashflow_amount(
    ctx: *mut Context,
    id: u64,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(out, c.get::<Shared<dyn CashFlow>>(id)?.amount()?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cashflow_date(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(
                out,
                c.get::<Shared<dyn CashFlow>>(id)?.date().serial_number(),
            )
        })
    }
}
/// include_settlement: -1 unset, 0 false, 1 true. Date zero means unset.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_leg_npv(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    settings_id: u64,
    include_settlement: i32,
    settlement: i32,
    npv_date: i32,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let include = match include_settlement {
                -1 => None,
                0 => Some(false),
                1 => Some(true),
                _ => return Err(BindingError::invalid("invalid settlement flag")),
            };
            let settlement = if settlement == 0 {
                None
            } else {
                Some(date(settlement)?)
            };
            let npv_date = if npv_date == 0 {
                None
            } else {
                Some(date(npv_date)?)
            };
            let curve = curve(c, discount)?.current_link()?;
            output(
                out,
                CashFlows::npv(
                    &c.get::<Leg>(id)?,
                    &*curve,
                    &*settings(c, settings_id)?,
                    include,
                    settlement,
                    npv_date,
                )?,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::cashflows::SimpleCashFlow;
    use libitofin::shared::shared;
    use libitofin::time::date::{Date, Month};
    use std::ptr::null_mut;
    #[test]
    fn generic_leg_negative_index_and_cashflow_lifetime() {
        let mut c = Context::new();
        let d = Date::new(1, Month::June, 2030);
        let flows: Leg =
            vec![shared(SimpleCashFlow::new(123.45, d).unwrap()) as Shared<dyn CashFlow>];
        let id = c.insert(flows).unwrap();
        let mut item = 0;
        let mut count = 0;
        unsafe {
            assert_eq!(itofin_leg_count(&mut c, id, &mut count, null_mut()), 0);
            assert_eq!(count, 1);
            assert_eq!(
                itofin_leg_item(&mut c, id, -2, &mut item, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(itofin_leg_item(&mut c, id, -1, &mut item, null_mut()), 0);
            assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
            let mut amount = 0.;
            assert_eq!(
                itofin_cashflow_amount(&mut c, item, &mut amount, null_mut()),
                0
            );
            assert_eq!(amount, 123.45);
            let mut serial = 0;
            assert_eq!(
                itofin_cashflow_date(&mut c, item, &mut serial, null_mut()),
                0
            );
            assert_eq!(serial, d.serial_number());
            assert_eq!(itofin_handle_release(&mut c, item, null_mut()), 0);
            assert_eq!(
                itofin_cashflow_amount(&mut c, item, &mut amount, null_mut()),
                INVALID_HANDLE
            );
        }
    }
}
