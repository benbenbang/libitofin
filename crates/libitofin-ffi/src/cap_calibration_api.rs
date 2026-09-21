//! Cap helpers and concrete Hull-White cap lattice engines.
use crate::boundary::*;
use crate::rates_api::{curve, finite};
use libitofin::math::optimization::{
    endcriteria::EndCriteria, levenbergmarquardt::LevenbergMarquardt,
};
use libitofin::math::timegrid::TimeGrid;
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::shortrate::calibrationhelpers::CapHelper;
use libitofin::models::{HullWhite, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::capfloor::TreeCapFloorEngine;
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::termstructures::volatility::VolatilityType;
use libitofin::time::period::Period;

#[repr(C)]
pub struct ItofinCapHelperConfig {
    pub length: i32,
    pub length_unit: i32,
    pub volatility: u64,
    pub index: u64,
    pub fixed_frequency: i32,
    pub fixed_day_counter: u64,
    pub include_first_swaplet: u8,
    pub curve: u64,
    pub error_type: i32,
    /// 0 shifted lognormal, 1 normal.
    pub volatility_type: i32,
    pub shift: f64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid. Handles belong to the calling context and thread.
pub unsafe extern "C" fn itofin_cap_helper_new(
    ctx: *mut Context,
    a: ItofinCapHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if a.include_first_swaplet > 1 {
                return Err(BindingError::invalid("invalid include-first flag"));
            }
            let kind = match a.volatility_type {
                0 => VolatilityType::ShiftedLognormal,
                1 => VolatilityType::Normal,
                _ => return Err(BindingError::invalid("invalid cap volatility type")),
            };
            let helper = CapHelper::try_new(
                Period::new(a.length, crate::time_api::time_unit(a.length_unit)?),
                crate::market_api::quote(c, a.volatility)?,
                crate::indexes_api::ibor_index(c, a.index)?,
                crate::time_api::frequency(a.fixed_frequency)?,
                crate::time_api::day_counter(c, a.fixed_day_counter)?,
                a.include_first_swaplet != 0,
                curve(c, a.curve)?,
                crate::models_api::error_type(a.error_type)?,
                kind,
                finite(a.shift)?,
            )?;
            output(out, c.insert(shared_mut(helper))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid. The times array has times_len elements.
pub unsafe extern "C" fn itofin_tree_capfloor_engine_new(
    ctx: *mut Context,
    model: u64,
    steps: usize,
    times: *const f64,
    times_len: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let times = input_slice(times, times_len)?;
            let model = c.get::<SharedMut<HullWhite>>(model)?;
            let engine = if times.is_empty() {
                TreeCapFloorEngine::new(model, steps)?
            } else {
                if steps != 0 {
                    return Err(BindingError::invalid("choose steps or an explicit grid"));
                }
                for &t in times {
                    finite(t)?;
                }
                TreeCapFloorEngine::with_time_grid(model, TimeGrid::from_mandatory_times(times)?)?
            };
            output(out, c.insert(shared_mut(engine))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid. Handles belong to the calling context and thread.
pub unsafe extern "C" fn itofin_cap_helper_set_tree_engine(
    ctx: *mut Context,
    helper: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let helper = c.get::<SharedMut<CapHelper>>(helper)?;
            let engine =
                c.get::<SharedMut<TreeCapFloorEngine>>(engine)? as SharedMut<dyn PricingEngine>;
            helper.borrow_mut().base_mut().set_pricing_engine(engine);
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// Field 0 market value, 1 black price at volatility, 2 model value, 3 calibration error.
/// # Safety
/// Pointers must be aligned and valid. Handles belong to the calling context and thread.
pub unsafe extern "C" fn itofin_cap_helper_value(
    ctx: *mut Context,
    helper: u64,
    field: i32,
    volatility: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let helper = c.get::<SharedMut<CapHelper>>(helper)?;
            let mut helper = helper.borrow_mut();
            let value = match field {
                0 => helper.market_value()?,
                1 => helper.black_price(finite(volatility)?)?,
                2 => helper.model_value()?,
                3 => helper.calibration_error()?,
                _ => return Err(BindingError::invalid("invalid cap helper field")),
            };
            output(out, value)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid. Buffer follows the crate's two-pass array contract.
pub unsafe extern "C" fn itofin_cap_helper_times(
    ctx: *mut Context,
    helper: u64,
    out: *mut f64,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let times = c
                .get::<SharedMut<CapHelper>>(helper)?
                .borrow()
                .mandatory_times()?;
            output(required, times.len())?;
            if out.is_null() && capacity == 0 {
                return Ok(());
            }
            if capacity < times.len() {
                return Err(BindingError::invalid("cap time buffer too small"));
            }
            check_ptr(out)?;
            std::ptr::copy_nonoverlapping(times.as_ptr(), out, times.len());
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned and valid. The helpers array has helpers_len elements.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_hullwhite_calibrate_caps(
    ctx: *mut Context,
    model: u64,
    helpers: *const u64,
    helpers_len: usize,
    method: u64,
    criteria: u64,
    steps: usize,
    fix_reversion: u8,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if fix_reversion > 1 {
                return Err(BindingError::invalid("invalid fix-reversion flag"));
            }
            let ids = input_slice(helpers, helpers_len)?;
            if ids.is_empty() {
                return Err(BindingError::invalid("cap helpers must not be empty"));
            }
            let helpers = ids
                .iter()
                .map(|id| c.get::<SharedMut<CapHelper>>(*id))
                .collect::<BindingResult<Vec<_>>>()?;
            let model = c.get::<SharedMut<HullWhite>>(model)?;
            let method = c.get::<SharedMut<LevenbergMarquardt>>(method)?;
            let criteria = c.get::<EndCriteria>(criteria)?;
            let engine = shared_mut(TreeCapFloorEngine::new(model.clone(), steps)?)
                as SharedMut<dyn PricingEngine>;
            for helper in &helpers {
                helper
                    .borrow_mut()
                    .base_mut()
                    .set_pricing_engine(engine.clone());
            }
            let helpers = helpers
                .into_iter()
                .map(|h| h as SharedMut<dyn CalibrationHelper>)
                .collect::<Vec<_>>();
            calibrate(
                &model,
                &helpers,
                &mut *method.borrow_mut(),
                &criteria,
                None,
                Vec::new(),
                if fix_reversion != 0 {
                    vec![true, false]
                } else {
                    Vec::new()
                },
            )?;
            Ok(())
        })
    }
}
