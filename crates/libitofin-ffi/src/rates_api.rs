//! Fixed/floating instruments. All handles retain their native dependencies.
use crate::boundary::*;
use crate::time_api::{date, day_counter, time_unit};
use libitofin::handle::Handle;
use libitofin::indexes::{IborIndex, OvernightIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::{FixedVsFloatingSwap, MakeOis, MakeVanillaSwap, OvernightIndexedSwap, SwapType, VanillaSwap};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Date, period::Period, schedule::Schedule};
use libitofin::types::Real;

pub(crate) fn period(length:i32, unit:i32)->BindingResult<Period> { Ok(Period::new(length,time_unit(unit)?)) }
pub(crate) fn finite(value:Real)->BindingResult<Real> {
    if value.is_finite() {Ok(value)} else {Err(BindingError::invalid("non-finite rate or amount"))}
}
pub(crate) fn swap_type(value:i32)->BindingResult<SwapType> {
    match value {0=>Ok(SwapType::Payer),1=>Ok(SwapType::Receiver),_=>Err(BindingError::invalid("invalid swap type"))}
}
pub(crate) fn curve(c:&Context,id:u64)->BindingResult<Handle<dyn YieldTermStructure>> { c.get(id) }
pub(crate) fn settings(c:&Context,id:u64)->BindingResult<Shared<Settings<Date>>> {c.get(id)}

#[repr(C)]
pub struct ItofinVanillaSwapConfig {
    pub swap_type:i32,pub nominal:Real,pub fixed_schedule:u64,pub fixed_rate:Real,
    pub fixed_day_counter:u64,pub floating_schedule:u64,pub index:u64,pub spread:Real,
    pub floating_day_counter:u64,pub settings:u64,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_vanilla_swap_new(ctx:*mut Context,a:ItofinVanillaSwapConfig,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        let swap=VanillaSwap::new(swap_type(a.swap_type)?,finite(a.nominal)?,c.get::<Schedule>(a.fixed_schedule)?,
            finite(a.fixed_rate)?,day_counter(c,a.fixed_day_counter)?,c.get::<Schedule>(a.floating_schedule)?,
            c.get::<Shared<IborIndex>>(a.index)?,finite(a.spread)?,day_counter(c,a.floating_day_counter)?,None,settings(c,a.settings)?)?;
        output(out,c.insert(shared_mut(swap.into_fixed_vs_floating()))?)
    })}
}
/// Optional fields are selected by bits: rate=1, effective=2, nominal=4,
/// fixed tenor=8, day counter=16, payment lag=32, discount=64, averaging=128.
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ItofinMakeSwapConfig {
    pub tenor_length:i32,pub tenor_unit:i32,pub index:u64,pub settings:u64,
    pub flags:u32,pub fixed_rate:Real,pub forward_length:i32,pub forward_unit:i32,
    pub effective_date:i32,pub nominal:Real,pub fixed_length:i32,pub fixed_unit:i32,
    pub fixed_day_counter:u64,pub payment_lag:i32,pub discount:u64,pub averaging:i32,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_make_vanilla_swap(ctx:*mut Context,a:ItofinMakeSwapConfig,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        if a.flags & !31 != 0 {return Err(BindingError::invalid("invalid vanilla builder flags"));}
        let rate=if a.flags&1!=0 {Some(finite(a.fixed_rate)?)}else{None};
        let mut builder=MakeVanillaSwap::new(period(a.tenor_length,a.tenor_unit)?,c.get(a.index)?,rate,
            period(a.forward_length,a.forward_unit)?,settings(c,a.settings)?);
        if a.flags&2!=0 {builder=builder.with_effective_date(date(a.effective_date)?);}
        if a.flags&4!=0 {builder=builder.with_nominal(finite(a.nominal)?);}
        if a.flags&8!=0 {builder=builder.with_fixed_leg_tenor(period(a.fixed_length,a.fixed_unit)?);}
        if a.flags&16!=0 {builder=builder.with_fixed_leg_day_count(day_counter(c,a.fixed_day_counter)?);}
        output(out,c.insert(shared_mut(builder.build()?.into_fixed_vs_floating()))?)
    })}
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_make_ois(ctx:*mut Context,a:ItofinMakeSwapConfig,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        if a.flags & !231 != 0 {return Err(BindingError::invalid("invalid OIS builder flags"));}
        let rate=if a.flags&1!=0 {Some(finite(a.fixed_rate)?)}else{None};
        let mut builder=MakeOis::new(period(a.tenor_length,a.tenor_unit)?,c.get::<Shared<OvernightIndex>>(a.index)?,
            rate,period(a.forward_length,a.forward_unit)?,settings(c,a.settings)?);
        if a.flags&2!=0 {builder=builder.with_effective_date(date(a.effective_date)?);}
        if a.flags&4!=0 {builder=builder.with_nominal(finite(a.nominal)?);}
        if a.flags&32!=0 {builder=builder.with_payment_lag(a.payment_lag);}
        if a.flags&64!=0 {builder=builder.with_discounting_term_structure(curve(c,a.discount)?);}
        if a.flags&128!=0 {builder=builder.with_averaging_method(match a.averaging {
            0=>libitofin::cashflows::RateAveraging::Simple,1=>libitofin::cashflows::RateAveraging::Compound,
            _=>return Err(BindingError::invalid("invalid averaging method")),});}
        output(out,c.insert(shared_mut(builder.build()?))?)
    })}
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_vanilla_swap_set_engine(ctx:*mut Context,id:u64,discount:u64,settings_id:u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        let swap=c.get::<SharedMut<FixedVsFloatingSwap>>(id)?;
        let engine=shared_mut(DiscountingSwapEngine::new(curve(c,discount)?,None,None,None,settings(c,settings_id)?)) as SharedMut<dyn PricingEngine>;
        swap.borrow_mut().base_mut().set_pricing_engine(engine); Ok(())
    })}
}
/// Swap kind: 0 vanilla, 1 OIS. Field: 0 NPV, 1 fair rate, 2 nominal,
/// 3 fixed rate, 4 calculated flag (does not calculate), 5 calculate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swap_value(ctx:*mut Context,id:u64,kind:i32,field:i32,out:*mut Real,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        let value=match kind {
            0=>{let obj=c.get::<SharedMut<FixedVsFloatingSwap>>(id)?; let mut s=obj.borrow_mut();
                match field {0=>s.npv()?,1=>s.fair_rate()?,2=>s.nominal()?,3=>s.fixed_rate(),4=>u8::from(s.base().is_calculated()) as Real,
                    5=>{s.calculate()?;0.},_=>return Err(BindingError::invalid("invalid swap field"))}},
            1=>{let obj=c.get::<SharedMut<OvernightIndexedSwap>>(id)?; let mut s=obj.borrow_mut();
                match field {0=>s.npv()?,1=>s.fixed_vs_floating_mut().fair_rate()?,2=>s.fixed_vs_floating().nominal()?,
                    3=>s.fixed_vs_floating().fixed_rate(),4=>u8::from(s.base().is_calculated()) as Real,
                    5=>{s.calculate()?;0.},_=>return Err(BindingError::invalid("invalid swap field"))}},
            _=>return Err(BindingError::invalid("invalid swap kind")),};
        output(out,value)
    })}
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swap_results(ctx:*mut Context,id:u64,kind:i32,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        let result=match kind {
            0=>{let obj=c.get::<SharedMut<FixedVsFloatingSwap>>(id)?;let mut s=obj.borrow_mut();s.calculate()?;crate::results_api::snapshot(s.base())},
            1=>{let obj=c.get::<SharedMut<OvernightIndexedSwap>>(id)?;let mut s=obj.borrow_mut();s.calculate()?;crate::results_api::snapshot(s.base())},
            _=>return Err(BindingError::invalid("invalid swap kind")),};
        output(out,c.insert(result)?)
    })}
}
