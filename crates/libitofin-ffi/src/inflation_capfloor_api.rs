//! Year-on-year inflation cap/floor construction and valuation.
use crate::boundary::*;
use crate::inflation_api::{cpi_interpolation, yoy_index};
use crate::inflation_vol_api::yoy_engine;
use crate::rates_api::{curve, finite, period};
use crate::rates_options::cap_type;
use crate::settings_api::settings;
use crate::time_api::{bool_flag, calendar, convention, date, day_counter};
use libitofin::cashflows::YoYInflationCoupon;
use libitofin::instrument::Instrument;
use libitofin::instruments::{MakeYoYInflationCapFloor, YoYInflationCapFloor};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};

type Maker = Shared<dyn Fn() -> BindingResult<YoYInflationCapFloor>>;
/// flags: nominal=1, effective date=2, fixing days=4, forward start=8,
/// strike=16. Optional handles are zero; payment convention -1 is default.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinMakeYoYCapFloorConfig {
    pub kind: i32,
    pub index: u64,
    pub length: usize,
    pub calendar: u64,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub settings: u64,
    pub flags: u32,
    pub nominal: f64,
    pub effective_date: i32,
    pub payment_day_counter: u64,
    pub payment_convention: i32,
    pub fixing_days: u32,
    pub engine: u64,
    pub as_optionlet: u8,
    pub forward_length: i32,
    pub forward_unit: i32,
    pub exclude_first: u8,
    pub strike: f64,
    pub atm_curve: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_make_yoy_capfloor_new(
    ctx: *mut Context,
    a: ItofinMakeYoYCapFloorConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if a.length > i32::MAX as usize || (a.flags & 4 != 0 && a.fixing_days > i32::MAX as u32)
            {
                return Err(BindingError::invalid(
                    "length or fixing days exceed core integer range",
                ));
            }
            if a.flags & !31 != 0 {
                return Err(BindingError::invalid("invalid builder flags"));
            }
            let kind = cap_type(a.kind)?;
            let index = yoy_index(c, a.index)?;
            let calendar = calendar(c, a.calendar)?;
            let lag = period(a.lag_length, a.lag_unit)?;
            let interpolation = cpi_interpolation(a.interpolation)?;
            let settings = settings(c, a.settings)?;
            let nominal = if a.flags & 1 != 0 {
                Some(finite(a.nominal)?)
            } else {
                None
            };
            let effective = if a.flags & 2 != 0 {
                Some(date(a.effective_date)?)
            } else {
                None
            };
            let dc = if a.payment_day_counter == 0 {
                None
            } else {
                Some(day_counter(c, a.payment_day_counter)?)
            };
            let conv = if a.payment_convention == -1 {
                None
            } else {
                Some(convention(a.payment_convention)?)
            };
            let engine = if a.engine == 0 {
                None
            } else {
                Some(yoy_engine(c, a.engine)?)
            };
            let forward = if a.flags & 8 != 0 {
                Some(period(a.forward_length, a.forward_unit)?)
            } else {
                None
            };
            let strike = if a.flags & 16 != 0 {
                Some(finite(a.strike)?)
            } else {
                None
            };
            let atm = if a.atm_curve == 0 {
                None
            } else {
                Some(curve(c, a.atm_curve)?)
            };
            let optionlet = bool_flag(a.as_optionlet)?;
            let exclude = bool_flag(a.exclude_first)?;
            let maker = shared(move || {
                let mut m = MakeYoYInflationCapFloor::new(
                    kind,
                    index.clone(),
                    a.length,
                    calendar.clone(),
                    lag,
                    interpolation,
                    settings.clone(),
                );
                if let Some(v) = nominal {
                    m = m.with_nominal(v);
                }
                if let Some(v) = effective {
                    m = m.with_effective_date(v);
                }
                if let Some(v) = &dc {
                    m = m.with_payment_day_counter(v.clone());
                }
                if let Some(v) = conv {
                    m = m.with_payment_adjustment(v);
                }
                if a.flags & 4 != 0 {
                    m = m.with_fixing_days(a.fixing_days);
                }
                if let Some(v) = &engine {
                    m = m.with_pricing_engine(v.clone());
                }
                if optionlet {
                    m = m.as_optionlet(true);
                }
                if exclude {
                    m = m.with_first_caplet_excluded();
                }
                if let Some(v) = forward {
                    m = m.with_forward_start(v);
                }
                if let Some(v) = strike {
                    m = m.with_strike(v);
                }
                if let Some(v) = &atm {
                    m = m.with_atm_strike(v.clone());
                }
                Ok(m.build()?)
            }) as Maker;
            output(out, c.insert(maker)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_make_yoy_capfloor_build(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let instrument = c.get::<Maker>(id)?()?;
            output(out, c.insert(shared_mut(instrument))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_capfloor_new(
    ctx: *mut Context,
    kind: i32,
    coupons: *const u64,
    coupon_count: usize,
    caps: *const f64,
    cap_count: usize,
    floors: *const f64,
    floor_count: usize,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let coupons = input_slice(coupons, coupon_count)?
                .iter()
                .map(|id| c.get::<Shared<YoYInflationCoupon>>(*id))
                .collect::<BindingResult<Vec<_>>>()?;
            let caps = input_slice(caps, cap_count)?;
            let floors = input_slice(floors, floor_count)?;
            if caps.iter().chain(floors).any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("nonfinite strike"));
            }
            let instrument = YoYInflationCapFloor::new(
                cap_type(kind)?,
                coupons,
                caps.to_vec(),
                floors.to_vec(),
                settings(c, settings_id)?,
            )?;
            output(out, c.insert(shared_mut(instrument))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_capfloor_set_engine(
    ctx: *mut Context,
    id: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<YoYInflationCapFloor>>(id)?
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(yoy_engine(c, engine)?);
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_capfloor_query(
    ctx: *mut Context,
    id: u64,
    query: i32,
    discount: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<SharedMut<YoYInflationCapFloor>>(id)?;
            let result = match query {
                0 => v.borrow_mut().npv()?,
                1 => {
                    v.borrow_mut().calculate()?;
                    0.
                }
                2 => f64::from(v.borrow().base().is_calculated()),
                3 => v.borrow().yoy_leg().len() as f64,
                4 => f64::from(v.borrow().start_date()?.serial_number()),
                5 => f64::from(v.borrow().maturity_date()?.serial_number()),
                6 => v
                    .borrow()
                    .atm_rate(curve(c, discount)?.current_link()?.as_ref())?,
                _ => return Err(BindingError::invalid("invalid capfloor query")),
            };
            output(out, result)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_capfloor_results(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = c.get::<SharedMut<YoYInflationCapFloor>>(id)?;
            v.borrow_mut().calculate()?;
            let snapshot = crate::results_api::snapshot(v.borrow().base());
            output(out, c.insert(snapshot)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_yoy_capfloor_strikes(
    ctx: *mut Context,
    id: u64,
    floor: u8,
    out: *mut f64,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = c.get::<SharedMut<YoYInflationCapFloor>>(id)?;
            let v = v.borrow();
            let strikes = if bool_flag(floor)? {
                v.floor_rates()
            } else {
                v.cap_rates()
            };
            output(required, strikes.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < strikes.len() {
                return Err(BindingError::invalid("strike buffer too small"));
            }
            check_ptr(out)?;
            std::ptr::copy_nonoverlapping(strikes.as_ptr(), out, strikes.len());
            Ok(())
        })
    }
}
