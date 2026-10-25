//! Immutable CPI quoted-price surfaces and cumulative CPI cap/floor valuation.
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::inflation_api::{cpi_interpolation, zero_index};
use crate::options_api::option_type;
use crate::rates_api::{curve, finite, period};
use crate::settings_api::settings;
use crate::time_api::{adjust, advance, convention, date, day_counter};
use libitofin::instrument::Instrument;
use libitofin::instruments::{CpiCapFloor, CpiCapFloorConfig};
use libitofin::math::matrix::Matrix;
use libitofin::option::OptionType;
use libitofin::pricingengines::InterpolatingCpiCapFloorEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::cpicapfloortermpricesurface::{
    CpiCapFloorSurfaceConfig, CpiCapFloorTermPriceSurface,
};
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::{period::Period, timeunit::TimeUnit};

/// Prices are row-major, strike rows by maturity columns. Maturities are years.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinCpiPriceSurfaceConfig {
    pub nominal: f64,
    pub base_rate: f64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub calendar: u64,
    pub convention: i32,
    pub day_counter: u64,
    pub index: u64,
    pub interpolation: i32,
    pub discount: u64,
    pub cap_strikes: *const f64,
    pub cap_strike_count: usize,
    pub floor_strikes: *const f64,
    pub floor_strike_count: usize,
    pub maturity_years: *const i32,
    pub maturity_count: usize,
    pub cap_prices: *const f64,
    pub cap_price_count: usize,
    pub floor_prices: *const f64,
    pub floor_price_count: usize,
    pub settings: u64,
}
fn matrix(rows: usize, cols: usize, prices: &[f64]) -> BindingResult<Matrix> {
    if rows.checked_mul(cols) != Some(prices.len()) {
        return Err(BindingError::invalid("price matrix dimensions mismatch"));
    }
    let mut result = Matrix::with_size(rows, cols);
    for (i, row) in prices.chunks(cols).enumerate() {
        result.row_mut(i).copy_from_slice(row);
    }
    Ok(result)
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for stated lengths, with disjoint
/// outputs. Context and handles belong to this thread. Serialize all calls.
pub unsafe extern "C" fn itofin_cpi_price_surface_new(
    ctx: *mut Context,
    a: ItofinCpiPriceSurfaceConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if a.maturity_count < 2 || a.cap_strike_count < 2 || a.floor_strike_count < 2 {
                return Err(BindingError::invalid("not enough strikes or maturities"));
            }
            let cal = c.get::<NativeCalendar>(a.calendar)?;
            let settings = settings(c, a.settings)?;
            let today = settings
                .evaluation_date()
                .ok_or_else(|| BindingError::invalid("evaluation date not set"))?;
            date(today.serial_number())?;
            let reference = adjust(&cal, today, BusinessDayConvention::Following)?;
            let maturities = input_slice(a.maturity_years, a.maturity_count)?;
            for y in maturities {
                advance(
                    &cal,
                    reference,
                    *y,
                    TimeUnit::Years,
                    convention(a.convention)?,
                    false,
                )?;
            }
            let surface = CpiCapFloorTermPriceSurface::new(CpiCapFloorSurfaceConfig {
                nominal: finite(a.nominal)?,
                base_rate: finite(a.base_rate)?,
                observation_lag: period(a.lag_length, a.lag_unit)?,
                calendar: cal.inner,
                business_day_convention: convention(a.convention)?,
                day_counter: day_counter(c, a.day_counter)?,
                index: zero_index(c, a.index)?,
                interpolation: cpi_interpolation(a.interpolation)?,
                discount: curve(c, a.discount)?,
                cap_strikes: input_slice(a.cap_strikes, a.cap_strike_count)?.to_vec(),
                floor_strikes: input_slice(a.floor_strikes, a.floor_strike_count)?.to_vec(),
                maturities: input_slice(a.maturity_years, a.maturity_count)?
                    .iter()
                    .map(|y| Period::new(*y, TimeUnit::Years))
                    .collect(),
                cap_prices: matrix(
                    a.cap_strike_count,
                    a.maturity_count,
                    input_slice(a.cap_prices, a.cap_price_count)?,
                )?,
                floor_prices: matrix(
                    a.floor_strike_count,
                    a.maturity_count,
                    input_slice(a.floor_prices, a.floor_price_count)?,
                )?,
                settings,
            })?;
            output(out, c.insert(shared(surface))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and disjoint. Context and handles belong to
/// this thread. Serialize all calls including destruction.
pub unsafe extern "C" fn itofin_cpi_price_surface_price(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    maturity: i32,
    strike: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let surface = c.get::<Shared<CpiCapFloorTermPriceSurface>>(id)?;
            let date = date(maturity)?;
            let price = match option_type(kind)? {
                OptionType::Call => surface.cap_price(date, finite(strike)?)?,
                OptionType::Put => surface.floor_price(date, finite(strike)?)?,
            };
            output(out, finite(price)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and disjoint. Context and handles belong to
/// this thread. Serialize all calls including destruction.
pub unsafe extern "C" fn itofin_cpi_capfloor_engine_new(
    ctx: *mut Context,
    surface: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let engine = InterpolatingCpiCapFloorEngine::new(
                c.get::<Shared<CpiCapFloorTermPriceSurface>>(surface)?,
            );
            output(out, c.insert(shared_mut(engine))?)
        })
    }
}
/// Contract nominal/base CPI/start date do not rescale the quoted surface price.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinCpiCapFloorConfig {
    pub kind: i32,
    pub nominal: f64,
    pub start: i32,
    pub base_cpi: f64,
    pub maturity: i32,
    pub fixing_calendar: u64,
    pub fixing_convention: i32,
    pub payment_calendar: u64,
    pub payment_convention: i32,
    pub strike: f64,
    pub index: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub settings: u64,
    pub engine: u64,
}
#[derive(Clone)]
struct CpiInstrument {
    inner: SharedMut<CpiCapFloor>,
    fixing_calendar: NativeCalendar,
    payment_calendar: NativeCalendar,
}
fn guard(
    a: &CpiCapFloorConfig,
    fixing: &NativeCalendar,
    payment: &NativeCalendar,
) -> BindingResult<()> {
    date(a.maturity.serial_number())?;
    if a.observation_lag.length() < 0 {
        return Err(BindingError::invalid("negative observation lag"));
    }
    advance(
        fixing,
        a.maturity,
        -a.observation_lag.length(),
        a.observation_lag.units(),
        a.fixing_convention,
        false,
    )?;
    adjust(payment, a.maturity, a.payment_convention)?;
    Ok(())
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and disjoint. Context and handles belong to
/// this thread. Serialize all calls including destruction.
pub unsafe extern "C" fn itofin_cpi_capfloor_new(
    ctx: *mut Context,
    a: ItofinCpiCapFloorConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let fixing_calendar = c.get::<NativeCalendar>(a.fixing_calendar)?;
            let payment_calendar = c.get::<NativeCalendar>(a.payment_calendar)?;
            let config = CpiCapFloorConfig {
                option_type: option_type(a.kind)?,
                nominal: finite(a.nominal)?,
                start_date: date(a.start)?,
                base_cpi: finite(a.base_cpi)?,
                maturity: date(a.maturity)?,
                fixing_calendar: fixing_calendar.inner.clone(),
                fixing_convention: convention(a.fixing_convention)?,
                payment_calendar: payment_calendar.inner.clone(),
                payment_convention: convention(a.payment_convention)?,
                strike: finite(a.strike)?,
                index: zero_index(c, a.index)?,
                observation_lag: period(a.lag_length, a.lag_unit)?,
                interpolation: cpi_interpolation(a.interpolation)?,
                settings: settings(c, a.settings)?,
            };
            guard(&config, &fixing_calendar, &payment_calendar)?;
            let mut instrument = CpiCapFloor::new(config)?;
            instrument
                .set_pricing_engine(c.get::<SharedMut<InterpolatingCpiCapFloorEngine>>(a.engine)?);
            output(
                out,
                c.insert(CpiInstrument {
                    inner: shared_mut(instrument),
                    fixing_calendar,
                    payment_calendar,
                })?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and disjoint. Context and handles belong to
/// this thread. Serialize all calls including destruction.
pub unsafe extern "C" fn itofin_cpi_capfloor_npv(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let instrument = c.get::<CpiInstrument>(id)?;
            guard(
                instrument.inner.borrow().config(),
                &instrument.fixing_calendar,
                &instrument.payment_calendar,
            )?;
            let value = instrument.inner.borrow_mut().npv()?;
            output(out, finite(value)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and disjoint. Context and handles belong to
/// this thread. Serialize all calls including destruction.
pub unsafe extern "C" fn itofin_cpi_capfloor_dates(
    ctx: *mut Context,
    id: u64,
    fixing: *mut i32,
    payment: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(fixing)?;
            check_ptr(payment)?;
            let owner = c.get::<CpiInstrument>(id)?;
            let instrument = owner.inner.borrow();
            guard(
                instrument.config(),
                &owner.fixing_calendar,
                &owner.payment_calendar,
            )?;
            let f = instrument.fixing_date()?.serial_number();
            let p = instrument.pay_date()?.serial_number();
            output(fixing, f)?;
            output(payment, p)
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and disjoint. Context and handles belong to
/// this thread. Serialize all calls including destruction.
pub unsafe extern "C" fn itofin_cpi_price_surface_reference_date(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let surface = c.get::<Shared<CpiCapFloorTermPriceSurface>>(id)?;
            output(out, surface.reference_date().serial_number())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and disjoint. Context and handles belong to
/// this thread. Serialize all calls including destruction.
pub unsafe extern "C" fn itofin_cpi_capfloor_is_expired(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let instrument = c.get::<CpiInstrument>(id)?;
            let expired = instrument.inner.borrow().is_expired()?;
            output(out, i32::from(expired))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpi_capfloor_errors_preserve_outputs_and_context() {
        let mut c = Context::new();
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        let mut out = 12345_u64;
        let mut value = 456.0;
        let mut fixing = 1000;
        let mut payment = 2000;
        unsafe {
            let a = std::mem::zeroed::<ItofinCpiPriceSurfaceConfig>();
            assert_ne!(
                itofin_cpi_price_surface_new(&mut c, a, &mut out, &mut error),
                0
            );
            assert_eq!(out, 12345);
            assert_ne!(
                itofin_cpi_price_surface_price(&mut c, 999, 0, 45000, 0.03, &mut value, &mut error),
                0
            );
            assert_eq!(value, 456.0);
            assert_ne!(
                itofin_cpi_capfloor_engine_new(&mut c, 999, &mut out, &mut error),
                0
            );
            assert_eq!(out, 12345);
            let a = std::mem::zeroed::<ItofinCpiCapFloorConfig>();
            assert_ne!(itofin_cpi_capfloor_new(&mut c, a, &mut out, &mut error), 0);
            assert_eq!(out, 12345);
            assert_ne!(
                itofin_cpi_capfloor_npv(&mut c, 999, &mut value, &mut error),
                0
            );
            assert_eq!(value, 456.0);
            assert_ne!(
                itofin_cpi_capfloor_dates(&mut c, 999, &mut fixing, &mut payment, &mut error),
                0
            );
            assert_ne!(
                itofin_cpi_price_surface_reference_date(&mut c, 999, &mut fixing, &mut error),
                0
            );
            assert_ne!(
                itofin_cpi_capfloor_is_expired(&mut c, 999, &mut payment, &mut error),
                0
            );
            assert_eq!((fixing, payment), (1000, 2000));
        }
        let foreign = Context::new();
        let wrong_type = c.insert(17_i32).unwrap();
        let mut foreign = foreign;
        unsafe {
            assert_ne!(
                itofin_cpi_price_surface_reference_date(
                    &mut c,
                    wrong_type,
                    &mut fixing,
                    &mut error
                ),
                0
            );
            assert_ne!(
                itofin_cpi_capfloor_is_expired(&mut foreign, wrong_type, &mut payment, &mut error),
                0
            );
        }
        assert_eq!((fixing, payment), (1000, 2000));
        let id = c.insert(17_i32).unwrap();
        assert_eq!(c.get::<i32>(id).unwrap(), 17);
    }
    #[test]
    fn cpi_capfloor_misaligned_error_precedes_all_writes() {
        let mut c = Context::new();
        let mut storage = std::mem::MaybeUninit::<ItofinError>::uninit();
        let mut out = 12345_u64;
        let mut value = 456.0;
        let mut fixing = 1000;
        let mut payment = 2000;
        unsafe {
            let error = storage
                .as_mut_ptr()
                .cast::<u8>()
                .add(1)
                .cast::<ItofinError>();
            let a = std::mem::zeroed::<ItofinCpiPriceSurfaceConfig>();
            assert_ne!(itofin_cpi_price_surface_new(&mut c, a, &mut out, error), 0);
            assert_ne!(
                itofin_cpi_price_surface_price(&mut c, 999, 0, 45000, 0.03, &mut value, error),
                0
            );
            assert_ne!(
                itofin_cpi_capfloor_engine_new(&mut c, 999, &mut out, error),
                0
            );
            let a = std::mem::zeroed::<ItofinCpiCapFloorConfig>();
            assert_ne!(itofin_cpi_capfloor_new(&mut c, a, &mut out, error), 0);
            assert_ne!(itofin_cpi_capfloor_npv(&mut c, 999, &mut value, error), 0);
            assert_ne!(
                itofin_cpi_capfloor_dates(&mut c, 999, &mut fixing, &mut payment, error),
                0
            );
            assert_ne!(
                itofin_cpi_price_surface_reference_date(&mut c, 999, &mut fixing, error),
                0
            );
            assert_ne!(
                itofin_cpi_capfloor_is_expired(&mut c, 999, &mut payment, error),
                0
            );
        }
        assert_eq!(out, 12345);
        assert_eq!(value, 456.0);
        assert_eq!((fixing, payment), (1000, 2000));
        let id = c.insert(17_i32).unwrap();
        assert_eq!(c.get::<i32>(id).unwrap(), 17);
    }
}
