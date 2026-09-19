//! Rate option instruments and pricing engine attachment.
use crate::boundary::*;
use crate::cashflows_api::IborLegConfig;
use crate::rates_api::{finite, period, settings};
use crate::time_api::date;
use libitofin::exercise::{EuropeanExercise, Exercise};
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    CapFloor, CapFloorType, FixedVsFloatingSwap, MakeCapFloor, SettlementMethod, SettlementType,
    Swaption,
};
use libitofin::models::HullWhite;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::{
    BachelierSwaptionEngine, BlackCapFloorEngine, BlackSwaptionEngine, JamshidianSwaptionEngine,
};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::types::Real;

#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_european_exercise_new(
    ctx: *mut Context,
    serial: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                c.insert(shared(EuropeanExercise::new(date(serial)?)) as Shared<dyn Exercise>)?,
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
pub unsafe extern "C" fn itofin_swaption_new(
    ctx: *mut Context,
    swap: u64,
    exercise: u64,
    settlement_type: i32,
    settlement_method: i32,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let t = match settlement_type {
                0 => SettlementType::Physical,
                1 => SettlementType::Cash,
                _ => return Err(BindingError::invalid("invalid settlement type")),
            };
            let m = match settlement_method {
                0 => SettlementMethod::PhysicalOTC,
                1 => SettlementMethod::PhysicalCleared,
                2 => SettlementMethod::CollateralizedCashPrice,
                3 => SettlementMethod::ParYieldCurve,
                _ => return Err(BindingError::invalid("invalid settlement method")),
            };
            let s = Swaption::new(
                c.get::<SharedMut<FixedVsFloatingSwap>>(swap)?,
                c.get::<Shared<dyn Exercise>>(exercise)?,
                t,
                m,
                settings(c, settings_id)?,
            );
            output(out, c.insert(shared_mut(s))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_swaption_from_ois(
    ctx: *mut Context,
    swap: u64,
    exercise: u64,
    settlement_type: i32,
    settlement_method: i32,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let t = match settlement_type {
                0 => SettlementType::Physical,
                1 => SettlementType::Cash,
                _ => return Err(BindingError::invalid("invalid settlement type")),
            };
            let m = match settlement_method {
                0 => SettlementMethod::PhysicalOTC,
                1 => SettlementMethod::PhysicalCleared,
                2 => SettlementMethod::CollateralizedCashPrice,
                3 => SettlementMethod::ParYieldCurve,
                _ => return Err(BindingError::invalid("invalid settlement method")),
            };
            let s = Swaption::new(
                c.get::<crate::rates_api::NativeOis>(swap)?.0,
                c.get::<Shared<dyn Exercise>>(exercise)?,
                t,
                m,
                settings(c, settings_id)?,
            );
            output(out, c.insert(shared_mut(s))?)
        })
    }
}
pub(crate) fn cap_type(t: i32) -> BindingResult<CapFloorType> {
    match t {
        0 => Ok(CapFloorType::Cap),
        1 => Ok(CapFloorType::Floor),
        2 => Ok(CapFloorType::Collar),
        _ => Err(BindingError::invalid("invalid cap/floor type")),
    }
}
#[repr(C)]
pub struct ItofinCapFloorConfig {
    pub kind: i32,
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub index: u64,
    pub strike: Real,
    pub forward_length: i32,
    pub forward_unit: i32,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_capfloor_new(
    ctx: *mut Context,
    a: ItofinCapFloorConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let cap = MakeCapFloor::new(
                cap_type(a.kind)?,
                period(a.tenor_length, a.tenor_unit)?,
                crate::indexes_api::ibor_index(c, a.index)?,
                finite(a.strike)?,
                period(a.forward_length, a.forward_unit)?,
                settings(c, a.settings)?,
            )
            .build()?;
            output(out, c.insert(shared_mut(cap))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_capfloor_from_leg(
    ctx: *mut Context,
    kind: i32,
    leg: u64,
    caps: *const Real,
    ncaps: usize,
    floors: *const Real,
    nfloors: usize,
    settings_id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let caps = input_slice(caps, ncaps)?.to_vec();
            let floors = input_slice(floors, nfloors)?.to_vec();
            for &v in caps.iter().chain(&floors) {
                finite(v)?;
            }
            let leg = c.get::<IborLegConfig>(leg)?.coupons()?;
            let settings = settings(c, settings_id)?;
            let obj = match cap_type(kind)? {
                CapFloorType::Cap => CapFloor::cap(leg, caps, settings)?,
                CapFloorType::Floor => CapFloor::floor(leg, floors, settings)?,
                CapFloorType::Collar => CapFloor::collar(leg, caps, floors, settings)?,
            };
            output(out, c.insert(shared_mut(obj))?)
        })
    }
}
/// Kind: 0 swaption Black, 1 swaption Bachelier, 2 swaption HullWhite, 3 cap/floor Black.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_rate_option_set_engine(
    ctx: *mut Context,
    id: u64,
    engine_id: u64,
    kind: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if kind == 3 {
                let cap = c.get::<SharedMut<CapFloor>>(id)?;
                let engine = c.get::<SharedMut<BlackCapFloorEngine>>(engine_id)?
                    as SharedMut<dyn PricingEngine>;
                cap.borrow_mut().base_mut().set_pricing_engine(engine);
                return Ok(());
            }
            let option = c.get::<SharedMut<Swaption>>(id)?;
            let engine = match kind {
                0 => c.get::<SharedMut<BlackSwaptionEngine>>(engine_id)?
                    as SharedMut<dyn PricingEngine>,
                1 => c.get::<SharedMut<BachelierSwaptionEngine>>(engine_id)?
                    as SharedMut<dyn PricingEngine>,
                2 => shared_mut(JamshidianSwaptionEngine::new(
                    c.get::<SharedMut<HullWhite>>(engine_id)?,
                )) as SharedMut<dyn PricingEngine>,
                _ => return Err(BindingError::invalid("invalid rate option engine")),
            };
            option.borrow_mut().base_mut().set_pricing_engine(engine);
            Ok(())
        })
    }
}
/// Instrument kind 0 swaption, 1 cap/floor. Field 0 NPV, 1 calculated, 2 calculate, 3 coupon count (cap/floor only).
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_rate_option_value(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    field: i32,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = match kind {
                0 => {
                    let v = c.get::<SharedMut<Swaption>>(id)?;
                    let mut v = v.borrow_mut();
                    match field {
                        0 => v.npv()?,
                        1 => u8::from(v.base().is_calculated()) as Real,
                        2 => {
                            v.calculate()?;
                            0.
                        }
                        _ => return Err(BindingError::invalid("invalid swaption field")),
                    }
                }
                1 => {
                    let v = c.get::<SharedMut<CapFloor>>(id)?;
                    let mut v = v.borrow_mut();
                    match field {
                        0 => v.npv()?,
                        1 => u8::from(v.base().is_calculated()) as Real,
                        2 => {
                            v.calculate()?;
                            0.
                        }
                        3 => v.coupons().len() as Real,
                        _ => return Err(BindingError::invalid("invalid cap/floor field")),
                    }
                }
                _ => return Err(BindingError::invalid("invalid rate option kind")),
            };
            output(out, value)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_rate_option_results(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let result = match kind {
                0 => {
                    let v = c.get::<SharedMut<Swaption>>(id)?;
                    let mut v = v.borrow_mut();
                    v.calculate()?;
                    crate::results_api::snapshot(v.base())
                }
                1 => {
                    let v = c.get::<SharedMut<CapFloor>>(id)?;
                    let mut v = v.borrow_mut();
                    v.calculate()?;
                    crate::results_api::snapshot(v.base())
                }
                _ => return Err(BindingError::invalid("invalid rate option kind")),
            };
            output(out, c.insert(result)?)
        })
    }
}
/// Which 0 cap rates, 1 floor rates. Capacity zero queries required length.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_capfloor_rates(
    ctx: *mut Context,
    id: u64,
    which: i32,
    out: *mut Real,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(required)?;
            let v = c.get::<SharedMut<CapFloor>>(id)?;
            let v = v.borrow();
            let rates = match which {
                0 => v.cap_rates(),
                1 => v.floor_rates(),
                _ => return Err(BindingError::invalid("invalid strike side")),
            };
            output(required, rates.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < rates.len() {
                return Err(BindingError::invalid("strike buffer too small"));
            }
            if !rates.is_empty() {
                check_ptr(out)?;
                std::ptr::copy_nonoverlapping(rates.as_ptr(), out, rates.len());
            }
            Ok(())
        })
    }
}

/// Payer-only vanilla builder. Flags: 1 strike, 2 nominal, 4 fixing date,
/// 8 exercise date, 16 indexed coupons. Zero calendar keeps index conventions.
#[repr(C)]
pub struct ItofinMakeSwaptionConfig {
    pub index: u64,
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub flags: u32,
    pub strike: Real,
    pub nominal: Real,
    pub fixing_date: i32,
    pub exercise_date: i32,
    pub exercise_calendar: u64,
    pub option_convention: i32,
    pub settlement_type: i32,
    pub settlement_method: i32,
    pub indexed_coupons: u8,
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles must belong
/// to the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_make_swaption(
    ctx: *mut Context,
    a: ItofinMakeSwaptionConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if a.flags & !31 != 0 {
                return Err(BindingError::invalid("invalid swaption builder flags"));
            }
            let index = c.get::<Shared<libitofin::indexes::SwapIndex>>(a.index)?;
            let strike = if a.flags & 1 != 0 {
                Some(finite(a.strike)?)
            } else {
                None
            };
            let mut builder = if a.flags & 4 != 0 {
                libitofin::instruments::MakeSwaption::with_fixing_date(
                    index,
                    date(a.fixing_date)?,
                    strike,
                )
            } else {
                libitofin::instruments::MakeSwaption::new(
                    index,
                    period(a.tenor_length, a.tenor_unit)?,
                    strike,
                )
            };
            let settlement_type = match a.settlement_type {
                0 => SettlementType::Physical,
                1 => SettlementType::Cash,
                _ => return Err(BindingError::invalid("invalid settlement type")),
            };
            let settlement_method = match a.settlement_method {
                0 => SettlementMethod::PhysicalOTC,
                1 => SettlementMethod::PhysicalCleared,
                2 => SettlementMethod::CollateralizedCashPrice,
                3 => SettlementMethod::ParYieldCurve,
                _ => return Err(BindingError::invalid("invalid settlement method")),
            };
            builder = builder
                .with_settlement_type(settlement_type)
                .with_settlement_method(settlement_method)
                .with_option_convention(crate::time_api::convention(a.option_convention)?);
            if a.flags & 2 != 0 {
                builder = builder.with_nominal(finite(a.nominal)?);
            }
            if a.flags & 8 != 0 {
                builder = builder.with_exercise_date(date(a.exercise_date)?);
            }
            if a.exercise_calendar != 0 {
                builder = builder
                    .with_exercise_calendar(crate::time_api::calendar(c, a.exercise_calendar)?);
            }
            if a.flags & 16 != 0 {
                builder = builder
                    .with_indexed_coupons(Some(crate::time_api::bool_flag(a.indexed_coupons)?));
            }
            output(out, c.insert(shared_mut(builder.build()?))?)
        })
    }
}

/// Fields: 0 exercise-date serial, 1 underlying fixed rate, 2 underlying nominal.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles must belong
/// to the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_swaption_details(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let option = c.get::<SharedMut<Swaption>>(id)?;
            let option = option.borrow();
            let value = match field {
                0 => Real::from(option.exercise().last_date().serial_number()),
                1 => option.underlying().borrow().fixed_rate(),
                2 => option.underlying().borrow().nominal()?,
                _ => return Err(BindingError::invalid("invalid swaption field")),
            };
            output(out, value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rates_api::{ItofinMakeSwapConfig, itofin_make_vanilla_swap};
    use crate::rates_engines::{ItofinRateEngineConfig, itofin_rate_engine_new};
    use libitofin::handle::Handle;
    use libitofin::indexes::ibor::Euribor;
    use libitofin::interestrate::Compounding;
    use libitofin::quotes::SimpleQuote;
    use libitofin::settings::Settings;
    use libitofin::termstructures::{yields::FlatForward, yieldtermstructure::YieldTermStructure};
    use libitofin::time::{
        businessdayconvention::BusinessDayConvention,
        calendars::target::Target,
        date::{Date, Month},
        daycounters::{
            actual365fixed::Actual365Fixed,
            thirty360::{Convention, Thirty360},
        },
        frequency::Frequency,
        timeunit::TimeUnit,
    };
    use std::ptr::null_mut;
    /// QuantLib swaption.cpp testCachedValue, both par/indexed coupon arms.
    #[test]
    fn test_cached_value_across_c_boundary_and_dependency_release() {
        for (at_par, expected) in [(true, 0.036418158579), (false, 0.036421429684)] {
            let mut c = Context::new();
            let cal = Target::new();
            let settings = shared(Settings::new());
            let today = Date::new(13, Month::March, 2002);
            settings.set_evaluation_date(today);
            settings.set_using_at_par_coupons(at_par);
            let settle = cal.advance(
                today,
                2,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            let exercise = cal.advance(
                settle,
                5,
                TimeUnit::Years,
                BusinessDayConvention::Following,
                false,
            );
            let start = cal.advance(
                exercise,
                2,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            let curve = Handle::new(shared(FlatForward::with_rate(
                settle,
                0.05,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);
            let index = c
                .insert(crate::indexes_api::NativeIbor::builtin(shared(
                    Euribor::six_months(curve.clone(), settings.clone()),
                )))
                .unwrap();
            let discount = c.insert(curve).unwrap();
            let settings_id = c.insert(settings).unwrap();
            let fixed_dc = c
                .insert(Thirty360::with_convention(Convention::BondBasis))
                .unwrap();
            let vol_dc = c.insert(Actual365Fixed::new()).unwrap();
            let quote = c.insert(shared(SimpleQuote::new(0.2))).unwrap();
            let a = ItofinMakeSwapConfig {
                tenor_length: 10,
                tenor_unit: 3,
                index,
                settings: settings_id,
                flags: 27,
                fixed_rate: 0.06,
                forward_length: 0,
                forward_unit: 0,
                effective_date: start.serial_number(),
                nominal: 0.,
                fixed_length: 1,
                fixed_unit: 3,
                fixed_day_counter: fixed_dc,
                payment_lag: 0,
                discount: 0,
                averaging: 0,
            };
            let (mut swap, mut ex, mut option, mut engine) = (0, 0, 0, 0);
            let mut npv = 0.;
            unsafe {
                assert_eq!(
                    itofin_make_vanilla_swap(&mut c, a, &mut swap, null_mut()),
                    0
                );
                assert_eq!(
                    itofin_european_exercise_new(
                        &mut c,
                        exercise.serial_number(),
                        &mut ex,
                        null_mut()
                    ),
                    0
                );
                assert_eq!(
                    itofin_swaption_new(
                        &mut c,
                        swap,
                        ex,
                        0,
                        0,
                        settings_id,
                        &mut option,
                        null_mut()
                    ),
                    0
                );
                assert_eq!(
                    itofin_rate_option_value(&mut c, option, 0, 0, &mut npv, null_mut()),
                    CORE_ERROR
                );
                let cfg = ItofinRateEngineConfig {
                    kind: 0,
                    discount,
                    volatility: quote,
                    settings: settings_id,
                    day_counter: vol_dc,
                    displacement: 0.,
                    cash_annuity_model: 0,
                    flat: 1,
                    has_displacement: 1,
                };
                assert_eq!(
                    itofin_rate_engine_new(&mut c, cfg, &mut engine, null_mut()),
                    0
                );
                assert_eq!(
                    itofin_rate_option_set_engine(&mut c, option, engine, 0, null_mut()),
                    0
                );
                for id in [
                    swap,
                    ex,
                    engine,
                    discount,
                    index,
                    quote,
                    settings_id,
                    fixed_dc,
                    vol_dc,
                ] {
                    assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
                }
                assert_eq!(
                    itofin_rate_option_value(&mut c, option, 0, 0, &mut npv, null_mut()),
                    0
                );
                assert!((npv - expected).abs() <= 1e-12, "{npv} vs {expected}");
                let mut snapshot = 0;
                assert_eq!(
                    itofin_rate_option_results(&mut c, option, 0, &mut snapshot, null_mut()),
                    0
                );
                assert_eq!(
                    c.get::<crate::results_api::ResultsSnapshot>(snapshot)
                        .unwrap()
                        .npv,
                    Some(npv)
                );
                assert_eq!(
                    itofin_rate_option_value(&mut c, option, 1, 0, &mut npv, null_mut()),
                    INVALID_HANDLE
                );
            }
        }
    }
}
