//! Multiplicative price and Kerkhof seasonality factors.
use crate::boundary::*;
use crate::inflation_curves_api::frequency_code;
use crate::time_api::{date, frequency};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::inflation::seasonality::{
    KerkhofSeasonality, MultiplicativePriceSeasonality, Seasonality,
};
use libitofin::time::date::Date;
use libitofin::time::frequency::Frequency;

#[derive(Clone)]
enum NativeSeasonality {
    Multiplicative(Shared<MultiplicativePriceSeasonality>),
    Kerkhof(Shared<KerkhofSeasonality>),
}

impl NativeSeasonality {
    fn shared(self) -> Shared<dyn Seasonality> {
        match self {
            Self::Multiplicative(value) => value,
            Self::Kerkhof(value) => value,
        }
    }

    fn seasonality_base_date(&self) -> Date {
        match self {
            Self::Multiplicative(value) => value.seasonality_base_date(),
            Self::Kerkhof(value) => value.seasonality_base_date(),
        }
    }

    fn frequency(&self) -> Frequency {
        match self {
            Self::Multiplicative(value) => value.frequency(),
            Self::Kerkhof(value) => value.frequency(),
        }
    }

    fn seasonality_factors(&self) -> &[f64] {
        match self {
            Self::Multiplicative(value) => value.seasonality_factors(),
            Self::Kerkhof(value) => value.seasonality_factors(),
        }
    }

    fn seasonality_factor(&self, date: Date) -> libitofin::errors::QlResult<f64> {
        match self {
            Self::Multiplicative(value) => value.seasonality_factor(date),
            Self::Kerkhof(value) => value.seasonality_factor(date),
        }
    }
}

pub(crate) fn seasonality(context: &Context, id: u64) -> BindingResult<Shared<dyn Seasonality>> {
    Ok(context.get::<NativeSeasonality>(id)?.shared())
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_seasonality_new(
    ctx: *mut Context,
    base: i32,
    freq: i32,
    factors: *const f64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let values = input_slice(factors, n)?.to_vec();
            if values.iter().any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("nonfinite seasonality factor"));
            }
            let p = shared(MultiplicativePriceSeasonality::new(
                date(base)?,
                frequency(freq)?,
                values,
            )?);
            output(out, c.insert(NativeSeasonality::Multiplicative(p))?)
        })
    }
}
/// query 0 base date serial, 1 frequency, 2 factor at date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_seasonality_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let p = c.get::<NativeSeasonality>(id)?;
            output(
                out,
                match query {
                    0 => p.seasonality_base_date().serial_number() as f64,
                    1 => frequency_code(p.frequency())? as f64,
                    2 => p.seasonality_factor(date(serial)?)?,
                    _ => return Err(BindingError::invalid("unknown seasonality query")),
                },
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_seasonality_factors(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    capacity: usize,
    count: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(count)?;
            let p = c.get::<NativeSeasonality>(id)?;
            let values = p.seasonality_factors();
            output(count, values.len())?;
            if capacity == 0 {
                return Ok(());
            }
            check_ptr(out)?;
            if capacity < values.len() {
                return Err(BindingError::invalid("factor buffer too small"));
            }
            for (i, v) in values.iter().enumerate() {
                output(out.add(i), *v)?;
            }
            Ok(())
        })
    }
}

/// Creates twelve-factor monthly Kerkhof seasonality for zero inflation curves.
/// Existing seasonality inspectors and curve setters accept the returned handle.
///
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_kerkhof_seasonality_new(
    ctx: *mut Context,
    base: i32,
    factors: *const f64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let values = input_slice(factors, n)?.to_vec();
            if values.iter().any(|value| !value.is_finite()) {
                return Err(BindingError::invalid("nonfinite seasonality factor"));
            }
            let value = shared(KerkhofSeasonality::new(date(base)?, values)?);
            output(out, c.insert(NativeSeasonality::Kerkhof(value))?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::{null, null_mut};

    #[test]
    fn kerkhof_native_boundary_validates_shapes_pointers_and_handles() {
        unsafe {
            let mut context = Context::new();
            let mut other = Context::new();
            let base = Date::new(1, libitofin::time::date::Month::January, 2007).serial_number();
            let mut id = 0;
            let mut factors = [1.0; 12];
            assert_eq!(
                itofin_kerkhof_seasonality_new(
                    &mut context,
                    base,
                    factors.as_ptr(),
                    12,
                    null_mut(),
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_kerkhof_seasonality_new(&mut context, base, null(), 12, &mut id, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(id, 0);
            for count in [0, 11] {
                assert_eq!(
                    itofin_kerkhof_seasonality_new(
                        &mut context,
                        base,
                        factors.as_ptr(),
                        count,
                        &mut id,
                        null_mut()
                    ),
                    CORE_ERROR
                );
                assert_eq!(id, 0);
            }
            factors[1] = f64::NAN;
            assert_eq!(
                itofin_kerkhof_seasonality_new(
                    &mut context,
                    base,
                    factors.as_ptr(),
                    12,
                    &mut id,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            factors[1] = 1.004;
            assert_eq!(
                itofin_kerkhof_seasonality_new(
                    &mut context,
                    base,
                    factors.as_ptr(),
                    12,
                    &mut id,
                    null_mut()
                ),
                0
            );
            factors[1] = 9.0;
            let mut value = 0.0;
            let february =
                Date::new(1, libitofin::time::date::Month::February, 2008).serial_number();
            assert_eq!(
                itofin_seasonality_value(&mut context, id, 2, february, &mut value, null_mut()),
                0
            );
            assert_eq!(value, 1.004);
            assert_ne!(value, factors[1]);
            assert_eq!(
                itofin_seasonality_value(&mut other, id, 2, february, &mut value, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(
                itofin_seasonality_value(&mut context, id, 3, february, &mut value, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_seasonality_value(&mut context, id, 2, 0, &mut value, null_mut()),
                INVALID_ARGUMENT
            );
            let mut count = 0;
            let mut buffer = [0.0; 11];
            assert_eq!(
                itofin_seasonality_factors(
                    &mut context,
                    id,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut count,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(count, 12);
            let retained = seasonality(&context, id).unwrap();
            assert_eq!(itofin_handle_release(&mut context, id, null_mut()), 0);
            assert_eq!(Shared::strong_count(&retained), 1);
            assert_eq!(
                itofin_seasonality_value(&mut context, id, 2, february, &mut value, null_mut()),
                INVALID_HANDLE
            );
        }
    }
}
