//! Extended optionlet stripping and ATM cap curves.
use crate::boundary::*;
use crate::market_api::quote;
use crate::settings_api::settings;
use crate::smile_api::volatility_type;
use crate::time_api::{bool_flag, calendar, convention, date, day_counter, time_unit};
use libitofin::handle::Handle;
use libitofin::indexes::iborindex::OvernightIndex;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    CapFloorTermVolCurve, CapFloorTermVolSurface, CapFloorTermVolatilityStructure,
    OptionletStripper1, OptionletStripper2, OptionletStripperOptions, OptionletVolatilityStructure,
    SmileSection, StrippedOptionletBase,
};
use libitofin::time::period::Period;

/// Extended configuration; the original stripping ABI remains unchanged.
#[repr(C)]
pub struct ItofinOptionletStripperConfig {
    pub surface: u64,
    pub index: u64,
    pub discount: u64,
    pub volatility_type: i32,
    pub accuracy: f64,
    pub max_iterations: u32,
    pub displacement: f64,
    pub frequency_length: i32,
    pub frequency_unit: i32,
    pub has_frequency: u8,
    pub switch_strike: f64,
    pub has_switch_strike: u8,
    pub dont_throw: u8,
    pub overnight: u8,
}

/// # Safety
/// Follow the crate C caller contract; configuration and output must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_stripper_new_with_options(
    ctx: *mut Context,
    cfg: *const ItofinOptionletStripperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(cfg)?;
            check_ptr(out)?;
            let x = &*cfg;
            let options = OptionletStripperOptions {
                switch_strike: if bool_flag(x.has_switch_strike)? {
                    Some(x.switch_strike)
                } else {
                    None
                },
                dont_throw: bool_flag(x.dont_throw)?,
            };
            let frequency = if bool_flag(x.has_frequency)? {
                Some(Period::new(
                    x.frequency_length,
                    time_unit(x.frequency_unit)?,
                ))
            } else {
                None
            };
            let discount = if x.discount == 0 {
                Handle::empty()
            } else {
                c.get(x.discount)?
            };
            let surface = c.get::<Shared<CapFloorTermVolSurface>>(x.surface)?;
            let kind = volatility_type(x.volatility_type)?;
            let stripper = if bool_flag(x.overnight)? {
                OptionletStripper1::new_overnight(
                    surface,
                    c.get::<Shared<OvernightIndex>>(x.index)?,
                    discount,
                    x.accuracy,
                    x.max_iterations,
                    kind,
                    x.displacement,
                    frequency,
                    options,
                )?
            } else {
                OptionletStripper1::new_with_options(
                    surface,
                    crate::indexes_api::ibor_index(c, x.index)?,
                    discount,
                    x.accuracy,
                    x.max_iterations,
                    kind,
                    x.displacement,
                    frequency,
                    options,
                )?
            };
            output(out, c.insert(shared(stripper))?)
        })
    }
}

/// Live ATM cap term-volatility curve. Zero settings selects a fixed reference date.
#[repr(C)]
pub struct ItofinCapFloorTermVolCurveConfig {
    pub reference_date: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub convention: i32,
    pub day_counter: u64,
    pub settings: u64,
    pub tenor_lengths: *const i32,
    pub tenor_units: *const i32,
    pub quotes: *const u64,
    pub count: usize,
}

/// # Safety
/// Follow the crate C caller contract; arrays must contain count entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_capfloor_term_vol_curve_new(
    ctx: *mut Context,
    cfg: *const ItofinCapFloorTermVolCurveConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(cfg)?;
            check_ptr(out)?;
            let x = &*cfg;
            let tenors = crate::volgrid_api::periods(x.tenor_lengths, x.tenor_units, x.count)?;
            let quotes = input_slice(x.quotes, x.count)?
                .iter()
                .map(|&id| quote(c, id))
                .collect::<BindingResult<Vec<_>>>()?;
            let cal = calendar(c, x.calendar)?;
            let bdc = convention(x.convention)?;
            let dc = day_counter(c, x.day_counter)?;
            let curve = if x.settings == 0 {
                CapFloorTermVolCurve::with_reference_date(
                    date(x.reference_date)?,
                    cal,
                    bdc,
                    tenors,
                    quotes,
                    dc,
                )?
            } else {
                CapFloorTermVolCurve::moving(
                    x.settlement_days,
                    cal,
                    bdc,
                    tenors,
                    quotes,
                    dc,
                    settings(c, x.settings)?,
                )?
            };
            output(out, c.insert(shared(curve))?)
        })
    }
}

/// # Safety
/// Follow the crate C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_capfloor_term_vol_curve_value(
    ctx: *mut Context,
    id: u64,
    length: i32,
    unit: i32,
    extrapolate: u8,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                c.get::<Shared<CapFloorTermVolCurve>>(id)?
                    .volatility_tenor(
                        Period::new(length, time_unit(unit)?),
                        0.0,
                        bool_flag(extrapolate)?,
                    )?,
            )
        })
    }
}

/// # Safety
/// Follow the crate C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_stripper2_new(
    ctx: *mut Context,
    stripper: u64,
    curve: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = OptionletStripper2::new(
                c.get::<Shared<OptionletStripper1>>(stripper)?,
                c.get::<Shared<CapFloorTermVolCurve>>(curve)?,
            )?;
            output(out, c.insert(shared(value))?)
        })
    }
}

pub(crate) fn stripped(c: &Context, id: u64) -> BindingResult<Shared<dyn StrippedOptionletBase>> {
    if let Ok(value) = c.get::<Shared<OptionletStripper1>>(id) {
        return Ok(value);
    }
    Ok(c.get::<Shared<OptionletStripper2>>(id)?)
}

/// Query 0 ATM curve times, 1 correction spreads, 2 ATM strikes, 3 ATM prices.
/// Pass null output/capacity zero to size the caller-owned buffer.
/// # Safety
/// Follow the crate C caller contract; output must have capacity entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_completion_values(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    out: *mut f64,
    capacity: usize,
    written: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(written)?;
            let values = match kind {
                0 => c.get::<Shared<CapFloorTermVolCurve>>(id)?.option_times()?,
                1 => c.get::<Shared<OptionletStripper2>>(id)?.spreads_vol()?,
                2 => c
                    .get::<Shared<OptionletStripper2>>(id)?
                    .atm_cap_floor_strikes()?,
                3 => c
                    .get::<Shared<OptionletStripper2>>(id)?
                    .atm_cap_floor_prices()?,
                _ => return Err(BindingError::invalid("unknown optionlet array query")),
            };
            output(written, values.len())?;
            if out.is_null() && capacity == 0 {
                return Ok(());
            }
            check_ptr(out)?;
            if capacity < values.len() {
                return Err(BindingError::invalid("optionlet array buffer too small"));
            }
            for (i, value) in values.iter().enumerate() {
                out.add(i).write(*value);
            }
            Ok(())
        })
    }
}

/// Query kind 0 time, 1 date serial, 2 tenor.
/// # Safety
/// Follow the crate C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_smile_new(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    time: f64,
    date_serial: i32,
    length: i32,
    unit: i32,
    extrapolate: u8,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let surface = c
                .get::<Handle<dyn OptionletVolatilityStructure>>(id)?
                .current_link()?;
            let extra = bool_flag(extrapolate)?;
            let smile = match kind {
                0 => surface.smile_section(time, extra)?,
                1 => surface.smile_section_date(date(date_serial)?, extra)?,
                2 => surface.smile_section_tenor(Period::new(length, time_unit(unit)?), extra)?,
                _ => return Err(BindingError::invalid("unknown smile date kind")),
            };
            output(out, c.insert(smile)?)
        })
    }
}

/// Query kind 0 volatility, 1 variance, 2 exercise time.
/// # Safety
/// Follow the crate C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_optionlet_smile_value(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    strike: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let smile = c.get::<Shared<dyn SmileSection>>(id)?;
            let value = match kind {
                0 => smile.volatility(strike)?,
                1 => smile.variance(strike)?,
                2 => smile.exercise_time(),
                _ => return Err(BindingError::invalid("unknown smile query")),
            };
            output(out, value)
        })
    }
}

/// A compounded overnight cap/floor with index day count and Following payments.
#[repr(C)]
pub struct ItofinOvernightCapFloorConfig {
    pub kind: i32,
    pub schedule: u64,
    pub index: u64,
    pub settings: u64,
    pub nominal: f64,
    pub payment_lag: i32,
    pub payment_adjustment: i32,
    pub caps: *const f64,
    pub cap_count: usize,
    pub floors: *const f64,
    pub floor_count: usize,
}

/// # Safety
/// Follow the crate C caller contract; strike arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_overnight_capfloor_new(
    ctx: *mut Context,
    cfg: *const ItofinOvernightCapFloorConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(cfg)?;
            check_ptr(out)?;
            let x = &*cfg;
            if !x.nominal.is_finite() {
                return Err(BindingError::invalid("nominal must be finite"));
            }
            let caps = input_slice(x.caps, x.cap_count)?.to_vec();
            let floors = input_slice(x.floors, x.floor_count)?.to_vec();
            if caps.iter().chain(&floors).any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("strikes must be finite"));
            }
            let coupons = libitofin::cashflows::OvernightLeg::new(
                c.get::<libitofin::time::schedule::Schedule>(x.schedule)?,
                c.get::<Shared<OvernightIndex>>(x.index)?,
            )
            .with_notional(x.nominal)
            .with_payment_lag(x.payment_lag)
            .with_payment_adjustment(convention(x.payment_adjustment)?)
            .coupons()?;
            let cap = libitofin::instruments::CapFloor::from_overnight(
                crate::rates_options::cap_type(x.kind)?,
                coupons,
                caps,
                floors,
                settings(c, x.settings)?,
            )?;
            output(out, c.insert(libitofin::shared::shared_mut(cap))?)
        })
    }
}
