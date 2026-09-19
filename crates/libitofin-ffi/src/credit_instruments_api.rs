//! CDS instruments, engines, and bootstrap helpers.
use crate::boundary::*;
use crate::credit_api::CreditCurve;
use crate::time_api::{calendar, convention, date, day_counter, frequency, generation, time_unit};
use libitofin::cashflow::CashFlow;
use libitofin::event::Event;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    CdsTerms, CreditDefaultSwap, MakeCreditDefaultSwap, PricingModel, ProtectionSide,
};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::MidPointCdsEngine;
use libitofin::pricingengines::credit::{
    AccrualBias, ForwardsInCouponPeriod, IsdaCdsEngine, NumericalFix,
};
use libitofin::quotes::SimpleQuote;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::credit::defaultprobabilityhelpers::{
    DefaultProbabilityHelper, SpreadCdsHelper,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Date, period::Period, schedule::Schedule};

fn flag(v: i32) -> BindingResult<bool> {
    match v {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("boolean must be 0 or 1")),
    }
}
fn side(v: i32) -> BindingResult<ProtectionSide> {
    match v {
        0 => Ok(ProtectionSide::Buyer),
        1 => Ok(ProtectionSide::Seller),
        _ => Err(BindingError::invalid("unknown protection side")),
    }
}
#[repr(C)]
pub struct ItofinSpreadCdsConfig {
    pub quote: u64,
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub settlement_days: i32,
    pub calendar: u64,
    pub frequency: i32,
    pub convention: i32,
    pub rule: i32,
    pub day_counter: u64,
    pub recovery: f64,
    pub discount: u64,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_spread_cds_helper_new(
    ctx: *mut Context,
    config: *const ItofinSpreadCdsConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            let helper = SpreadCdsHelper::new(
                Handle::new(c.get::<Shared<SimpleQuote>>(a.quote)?),
                Period::new(a.tenor_length, time_unit(a.tenor_unit)?),
                a.settlement_days,
                calendar(c, a.calendar)?,
                frequency(a.frequency)?,
                convention(a.convention)?,
                generation(a.rule)?,
                day_counter(c, a.day_counter)?,
                a.recovery,
                c.get::<Handle<dyn YieldTermStructure>>(a.discount)?,
                c.get::<Shared<Settings<Date>>>(a.settings)?,
            )? as Shared<dyn DefaultProbabilityHelper>;
            output(out, c.insert(helper)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_default_helper_dates(
    ctx: *mut Context,
    id: u64,
    pillar: *mut i32,
    latest: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(pillar)?;
            check_ptr(latest)?;
            let h = c.get::<Shared<dyn DefaultProbabilityHelper>>(id)?;
            output(pillar, h.pillar_date().serial_number())?;
            output(latest, h.latest_date().serial_number())
        })
    }
}
/// kind 0 = midpoint, 1 = ISDA. Fidelity enums follow Python declaration order.
#[repr(C)]
pub struct ItofinCdsEngineConfig {
    pub probability: u64,
    pub discount: u64,
    pub settings: u64,
    pub recovery: f64,
    pub kind: i32,
    pub numerical_fix: i32,
    pub accrual_bias: i32,
    pub forwards: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cds_engine_new(
    ctx: *mut Context,
    config: *const ItofinCdsEngineConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            if !a.recovery.is_finite() || !(0.0..=1.0).contains(&a.recovery) {
                return Err(BindingError::invalid("recovery must be in [0,1]"));
            }
            let p = c.get::<CreditCurve>(a.probability)?.handle;
            let d = c.get::<Handle<dyn YieldTermStructure>>(a.discount)?;
            let s = c.get::<Shared<Settings<Date>>>(a.settings)?;
            let engine: SharedMut<dyn PricingEngine> = match a.kind {
                0 => shared_mut(MidPointCdsEngine::new(p, a.recovery, d, None, s)),
                1 => shared_mut(IsdaCdsEngine::new(p, a.recovery, d, None, s).with_fidelity(
                    match a.numerical_fix {
                        0 => NumericalFix::NoFix,
                        1 => NumericalFix::Taylor,
                        _ => return Err(BindingError::invalid("unknown numerical fix")),
                    },
                    match a.accrual_bias {
                        0 => AccrualBias::HalfDayBias,
                        1 => AccrualBias::NoBias,
                        _ => return Err(BindingError::invalid("unknown accrual bias")),
                    },
                    match a.forwards {
                        0 => ForwardsInCouponPeriod::Flat,
                        1 => ForwardsInCouponPeriod::Piecewise,
                        _ => return Err(BindingError::invalid("unknown forwards mode")),
                    },
                )),
                _ => return Err(BindingError::invalid("unknown CDS engine")),
            };
            output(out, c.insert(engine)?)
        })
    }
}
#[repr(C)]
pub struct ItofinCdsConfig {
    pub side: i32,
    pub notional: f64,
    pub spread: f64,
    pub schedule: u64,
    pub convention: i32,
    pub day_counter: u64,
    pub settings: u64,
    pub protection_start: i32,
    pub settles_accrual: i32,
    pub pays_at_default_time: i32,
    pub rebates_accrual: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cds_new(
    ctx: *mut Context,
    config: *const ItofinCdsConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            let terms = CdsTerms {
                settles_accrual: flag(a.settles_accrual)?,
                pays_at_default_time: flag(a.pays_at_default_time)?,
                protection_start: if a.protection_start == 0 {
                    None
                } else {
                    Some(date(a.protection_start)?)
                },
                rebates_accrual: flag(a.rebates_accrual)?,
                ..CdsTerms::default()
            };
            let cds = CreditDefaultSwap::with_terms(
                side(a.side)?,
                a.notional,
                a.spread,
                c.get::<Schedule>(a.schedule)?,
                convention(a.convention)?,
                day_counter(c, a.day_counter)?,
                terms,
                c.get::<Shared<Settings<Date>>>(a.settings)?,
            )?;
            output(out, c.insert(shared_mut(cds))?)
        })
    }
}
#[repr(C)]
pub struct ItofinMakeCdsConfig {
    pub term_date: i32,
    pub running_spread: f64,
    pub settings: u64,
    pub nominal: f64,
    pub upfront: f64,
    pub has_upfront: i32,
    pub side: i32,
    pub trade_date: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_make_cds(
    ctx: *mut Context,
    config: *const ItofinMakeCdsConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            let mut maker = MakeCreditDefaultSwap::from_term_date(
                date(a.term_date)?,
                a.running_spread,
                c.get::<Shared<Settings<Date>>>(a.settings)?,
            )
            .with_nominal(a.nominal)
            .with_side(side(a.side)?);
            if flag(a.has_upfront)? {
                maker = maker.with_upfront_rate(a.upfront);
            }
            if a.trade_date != 0 {
                maker = maker.with_trade_date(date(a.trade_date)?);
            }
            output(out, c.insert(shared_mut(maker.build()?))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cds_set_engine(
    ctx: *mut Context,
    id: u64,
    engine: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<CreditDefaultSwap>>(id)?
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(c.get::<SharedMut<dyn PricingEngine>>(engine)?);
            Ok(())
        })
    }
}
/// query: 0 NPV, 1 fair spread, 2 fair upfront, 3 notional, 4 coupon NPV,
/// 5 default NPV, 6 calculate, 7 is-calculated.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cds_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let cds = c.get::<SharedMut<CreditDefaultSwap>>(id)?;
            let mut cds = cds.borrow_mut();
            let v = match query {
                0 => cds.npv()?,
                1 => cds.fair_spread()?,
                2 => cds.fair_upfront()?,
                3 => cds.notional(),
                4 => cds.coupon_leg_npv()?,
                5 => cds.default_leg_npv()?,
                6 => {
                    cds.calculate()?;
                    0.0
                }
                7 => {
                    if cds.base().is_calculated() {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => return Err(BindingError::invalid("unknown CDS query")),
            };
            output(out, v)
        })
    }
}
#[unsafe(no_mangle)]
/// Return the final premium coupon's accrual-end serial date.
///
/// # Safety
/// Pointers must be aligned, live and valid. Output must not overlap inputs.
/// The context and its handles must belong to the calling thread.
pub unsafe extern "C" fn itofin_cds_protection_end_date(
    ctx: *mut Context,
    id: u64,
    serial: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(serial)?;
            let cds = c.get::<SharedMut<CreditDefaultSwap>>(id)?;
            let date = cds.borrow().protection_end_date()?;
            output(serial, date.serial_number())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cds_rebate(
    ctx: *mut Context,
    id: u64,
    has: *mut i32,
    amount: *mut f64,
    serial: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(has)?;
            check_ptr(amount)?;
            check_ptr(serial)?;
            let cds = c.get::<SharedMut<CreditDefaultSwap>>(id)?;
            let cds = cds.borrow();
            if let Some(r) = cds.accrual_rebate() {
                let value = r.amount()?;
                output(amount, value)?;
                output(serial, Event::date(r.as_ref()).serial_number())?;
                output(has, 1)
            } else {
                output(amount, 0.0)?;
                output(serial, 0)?;
                output(has, 0)
            }
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_cds_implied_hazard(
    ctx: *mut Context,
    id: u64,
    target: f64,
    discount: u64,
    dc: u64,
    recovery: f64,
    accuracy: f64,
    model: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !accuracy.is_finite() || accuracy <= 0.0 {
                return Err(BindingError::invalid("accuracy must be positive"));
            }
            let m = match model {
                0 => PricingModel::Midpoint,
                1 => PricingModel::Isda,
                _ => return Err(BindingError::invalid("unknown pricing model")),
            };
            let value = c
                .get::<SharedMut<CreditDefaultSwap>>(id)?
                .borrow()
                .implied_hazard_rate(
                    target,
                    &c.get::<Handle<dyn YieldTermStructure>>(discount)?,
                    day_counter(c, dc)?,
                    recovery,
                    accuracy,
                    m,
                )?;
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
pub unsafe extern "C" fn itofin_cds_results(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let cds = c.get::<SharedMut<CreditDefaultSwap>>(id)?;
            let mut cds = cds.borrow_mut();
            cds.calculate()?;
            let snapshot = crate::results_api::snapshot(cds.base());
            output(out, c.insert(snapshot)?)
        })
    }
}

#[cfg(test)]
mod protection_end_tests {
    use super::*;

    #[test]
    fn protection_end_rejects_invalid_outputs_and_handles() {
        let mut ctx = Context::new();
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        let mut serial = -1;
        unsafe {
            assert_eq!(
                itofin_cds_protection_end_date(&mut ctx, 0, std::ptr::null_mut(), &mut error),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_cds_protection_end_date(&mut ctx, 0, &mut serial, &mut error),
                INVALID_HANDLE
            );
            let wrong_type = ctx.insert(42_u32).unwrap();
            assert_eq!(
                itofin_cds_protection_end_date(&mut ctx, wrong_type, &mut serial, &mut error),
                INVALID_HANDLE
            );
        }
        assert_eq!(serial, -1);
    }
}
