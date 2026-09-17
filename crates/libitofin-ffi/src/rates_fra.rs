//! Forward rate agreements retain index and discount handles independently.
use crate::boundary::*;
use crate::rates_api::{curve, finite};
use crate::time_api::date;
use libitofin::handle::Handle;
use libitofin::indexes::InterestRateIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::ForwardRateAgreement;
use libitofin::position::Position;
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::time::date::Date;
use libitofin::types::Real;

pub(crate) struct Fra {
    inner: ForwardRateAgreement,
    value: Date,
    maturity: Date,
}
#[repr(C)]
pub struct ItofinFraConfig {
    pub index: u64,
    pub value_date: i32,
    /// Zero derives the index maturity.
    pub maturity_date: i32,
    pub position: i32,
    pub strike: Real,
    pub notional: Real,
    pub discount: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_fra_new(
    ctx: *mut Context,
    a: ItofinFraConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let index = crate::indexes_api::ibor_index(c, a.index)?;
            let value = date(a.value_date)?;
            let pos = match a.position {
                0 => Position::Long,
                1 => Position::Short,
                _ => return Err(BindingError::invalid("invalid position")),
            };
            let discount = if a.discount == 0 {
                Handle::empty()
            } else {
                curve(c, a.discount)?
            };
            let maturity = if a.maturity_date == 0 {
                index.maturity_date(value)?
            } else {
                date(a.maturity_date)?
            };
            let inner = if a.maturity_date == 0 {
                ForwardRateAgreement::new(
                    index,
                    value,
                    pos,
                    finite(a.strike)?,
                    finite(a.notional)?,
                    discount,
                )?
            } else {
                ForwardRateAgreement::with_maturity(
                    index,
                    value,
                    maturity,
                    pos,
                    finite(a.strike)?,
                    finite(a.notional)?,
                    discount,
                )?
            };
            let maturity = inner
                .calendar()
                .adjust(maturity, inner.business_day_convention());
            output(
                out,
                c.insert(shared_mut(Fra {
                    inner,
                    value,
                    maturity,
                }))?,
            )
        })
    }
}
/// Field 0 NPV, 1 forward rate, 2 amount.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_fra_value(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let obj = c.get::<SharedMut<Fra>>(id)?;
            let mut a = obj.borrow_mut();
            let v = match field {
                0 => a.inner.npv()?,
                1 => a.inner.forward_rate()?.rate(),
                2 => a.inner.amount()?,
                _ => return Err(BindingError::invalid("invalid FRA field")),
            };
            output(out, v)
        })
    }
}
/// Field 0 value date, 1 adjusted maturity date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_fra_date(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let obj = c.get::<SharedMut<Fra>>(id)?;
            let a = obj.borrow();
            let d = match field {
                0 => a.value,
                1 => a.maturity,
                _ => return Err(BindingError::invalid("invalid FRA date field")),
            };
            output(out, d.serial_number())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::indexes::ibor::Euribor;
    use libitofin::interestrate::Compounding;
    use libitofin::settings::Settings;
    use libitofin::shared::{Shared, shared};
    use libitofin::termstructures::{yields::FlatForward, yieldtermstructure::YieldTermStructure};
    use libitofin::time::{date::Month, daycounters::actual360::Actual360, frequency::Frequency};
    use std::ptr::null_mut;
    /// ForwardRateAgreement closed-form oracle already pinned against QuantLib in core tests.
    #[test]
    fn par_forward_rate_amount_and_npv_match_the_closed_form() {
        let mut c = Context::new();
        let today = Date::new(15, Month::June, 2026);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        let flat = |rate| {
            Handle::new(shared(FlatForward::with_rate(
                today,
                rate,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        let index = c
            .insert(crate::indexes_api::NativeIbor::builtin(shared(
                Euribor::three_months(flat(0.04), settings),
            )))
            .unwrap();
        let discount = c.insert(flat(0.06)).unwrap();
        let start = Date::new(17, Month::August, 2026);
        let end = Date::new(17, Month::November, 2026);
        let dc = Actual360::new();
        let t = dc.year_fraction(start, end);
        let f = ((0.04 * t).exp() - 1.) / t;
        let amount = 100. * (f - 0.02) * t / (1. + f * t);
        for (position, sign) in [(0, 1.), (1, -1.)] {
            let a = ItofinFraConfig {
                index,
                value_date: start.serial_number(),
                maturity_date: end.serial_number(),
                position,
                strike: 0.02,
                notional: 100.,
                discount,
            };
            let mut id = 0;
            unsafe {
                assert_eq!(itofin_fra_new(&mut c, a, &mut id, null_mut()), 0);
            }
            for (field, expected) in [
                (
                    0,
                    sign * amount * (-0.06 * dc.year_fraction(today, start)).exp(),
                ),
                (1, f),
                (2, sign * amount),
            ] {
                let mut out = 0.;
                unsafe {
                    assert_eq!(itofin_fra_value(&mut c, id, field, &mut out, null_mut()), 0);
                }
                assert!((out - expected).abs() < 1e-12, "{out} vs {expected}");
            }
            let mut serial = 0;
            unsafe {
                assert_eq!(itofin_fra_date(&mut c, id, 1, &mut serial, null_mut()), 0);
            }
            assert_eq!(serial, end.serial_number());
        }
        let invalid = ItofinFraConfig {
            index,
            value_date: start.serial_number(),
            maturity_date: end.serial_number(),
            position: 99,
            strike: 0.02,
            notional: 100.,
            discount,
        };
        let mut id = 0;
        unsafe {
            assert_eq!(
                itofin_fra_new(&mut c, invalid, &mut id, null_mut()),
                INVALID_ARGUMENT
            );
        }
    }
}
