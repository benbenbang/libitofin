//! Rate option instruments and pricing engine attachment.
use crate::boundary::*;
use crate::rates_api::{finite,period,settings};
use crate::time_api::date;
use crate::cashflows_api::IborLegConfig;
use libitofin::exercise::{EuropeanExercise,Exercise};
use libitofin::instrument::Instrument;
use libitofin::instruments::{CapFloor,CapFloorType,MakeCapFloor,FixedVsFloatingSwap,SettlementMethod,SettlementType,Swaption};
use libitofin::models::HullWhite;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::{JamshidianSwaptionEngine,BlackCapFloorEngine,BlackSwaptionEngine,BachelierSwaptionEngine};
use libitofin::shared::{Shared,SharedMut,shared,shared_mut};
use libitofin::types::Real;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_european_exercise_new(ctx:*mut Context,serial:i32,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {check_ptr(out)?;output(out,c.insert(shared(EuropeanExercise::new(date(serial)?)) as Shared<dyn Exercise>)?)})}
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swaption_new(ctx:*mut Context,swap:u64,exercise:u64,settlement_type:i32,settlement_method:i32,settings_id:u64,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        let t=match settlement_type {0=>SettlementType::Physical,1=>SettlementType::Cash,_=>return Err(BindingError::invalid("invalid settlement type"))};
        let m=match settlement_method {0=>SettlementMethod::PhysicalOTC,1=>SettlementMethod::PhysicalCleared,
            2=>SettlementMethod::CollateralizedCashPrice,3=>SettlementMethod::ParYieldCurve,_=>return Err(BindingError::invalid("invalid settlement method"))};
        let s=Swaption::new(c.get::<SharedMut<FixedVsFloatingSwap>>(swap)?,c.get::<Shared<dyn Exercise>>(exercise)?,t,m,settings(c,settings_id)?);
        output(out,c.insert(shared_mut(s))?)
    })}
}
pub(crate) fn cap_type(t:i32)->BindingResult<CapFloorType> {match t {
    0=>Ok(CapFloorType::Cap),1=>Ok(CapFloorType::Floor),2=>Ok(CapFloorType::Collar),_=>Err(BindingError::invalid("invalid cap/floor type")),}}
#[repr(C)]
pub struct ItofinCapFloorConfig {
    pub kind:i32,pub tenor_length:i32,pub tenor_unit:i32,pub index:u64,pub strike:Real,
    pub forward_length:i32,pub forward_unit:i32,pub settings:u64,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_capfloor_new(ctx:*mut Context,a:ItofinCapFloorConfig,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        let cap=MakeCapFloor::new(cap_type(a.kind)?,period(a.tenor_length,a.tenor_unit)?,c.get(a.index)?,
            finite(a.strike)?,period(a.forward_length,a.forward_unit)?,settings(c,a.settings)?).build()?;
        output(out,c.insert(shared_mut(cap))?)
    })}
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_capfloor_from_leg(ctx:*mut Context,kind:i32,leg:u64,caps:*const Real,ncaps:usize,floors:*const Real,nfloors:usize,settings_id:u64,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;let caps=input_slice(caps,ncaps)?.to_vec();let floors=input_slice(floors,nfloors)?.to_vec();
        for &v in caps.iter().chain(&floors) {finite(v)?;}
        let leg=c.get::<IborLegConfig>(leg)?.coupons()?;let settings=settings(c,settings_id)?;
        let obj=match cap_type(kind)? {CapFloorType::Cap=>CapFloor::cap(leg,caps,settings)?,
            CapFloorType::Floor=>CapFloor::floor(leg,floors,settings)?,CapFloorType::Collar=>CapFloor::collar(leg,caps,floors,settings)?};
        output(out,c.insert(shared_mut(obj))?)
    })}
}
/// Kind: 0 swaption Black, 1 swaption Bachelier, 2 swaption HullWhite, 3 cap/floor Black.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_rate_option_set_engine(ctx:*mut Context,id:u64,engine_id:u64,kind:i32,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        if kind==3 {
            let cap=c.get::<SharedMut<CapFloor>>(id)?;
            let engine=c.get::<SharedMut<BlackCapFloorEngine>>(engine_id)? as SharedMut<dyn PricingEngine>;
            cap.borrow_mut().base_mut().set_pricing_engine(engine);return Ok(());
        }
        let option=c.get::<SharedMut<Swaption>>(id)?;
        let engine=match kind {
            0=>c.get::<SharedMut<BlackSwaptionEngine>>(engine_id)? as SharedMut<dyn PricingEngine>,
            1=>c.get::<SharedMut<BachelierSwaptionEngine>>(engine_id)? as SharedMut<dyn PricingEngine>,
            2=>shared_mut(JamshidianSwaptionEngine::new(c.get::<SharedMut<HullWhite>>(engine_id)?)) as SharedMut<dyn PricingEngine>,
            _=>return Err(BindingError::invalid("invalid rate option engine")),};
        option.borrow_mut().base_mut().set_pricing_engine(engine);Ok(())
    })}
}
/// Instrument kind 0 swaption, 1 cap/floor. Field 0 NPV, 1 calculated, 2 calculate, 3 coupon count (cap/floor only).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_rate_option_value(ctx:*mut Context,id:u64,kind:i32,field:i32,out:*mut Real,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;let value=match kind {
            0=>{let v=c.get::<SharedMut<Swaption>>(id)?;let mut v=v.borrow_mut();match field {
                0=>v.npv()?,1=>u8::from(v.base().is_calculated()) as Real,2=>{v.calculate()?;0.},_=>return Err(BindingError::invalid("invalid swaption field"))}},
            1=>{let v=c.get::<SharedMut<CapFloor>>(id)?;let mut v=v.borrow_mut();match field {
                0=>v.npv()?,1=>u8::from(v.base().is_calculated()) as Real,2=>{v.calculate()?;0.},3=>v.coupons().len() as Real,_=>return Err(BindingError::invalid("invalid cap/floor field"))}},
            _=>return Err(BindingError::invalid("invalid rate option kind")),};output(out,value)
    })}
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_rate_option_results(ctx:*mut Context,id:u64,kind:i32,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;let result=match kind {
            0=>{let v=c.get::<SharedMut<Swaption>>(id)?;let mut v=v.borrow_mut();v.calculate()?;crate::results_api::snapshot(v.base())},
            1=>{let v=c.get::<SharedMut<CapFloor>>(id)?;let mut v=v.borrow_mut();v.calculate()?;crate::results_api::snapshot(v.base())},
            _=>return Err(BindingError::invalid("invalid rate option kind")),};output(out,c.insert(result)?)
    })}
}
/// Which 0 cap rates, 1 floor rates. Capacity zero queries required length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_capfloor_rates(ctx:*mut Context,id:u64,which:i32,out:*mut Real,capacity:usize,required:*mut usize,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(required)?;let v=c.get::<SharedMut<CapFloor>>(id)?;let v=v.borrow();
        let rates=match which {0=>v.cap_rates(),1=>v.floor_rates(),_=>return Err(BindingError::invalid("invalid strike side"))};
        output(required,rates.len())?;if capacity==0 {return Ok(());}
        if capacity<rates.len(){return Err(BindingError::invalid("strike buffer too small"));}
        if !rates.is_empty(){check_ptr(out)?;std::ptr::copy_nonoverlapping(rates.as_ptr(),out,rates.len());}Ok(())
    })}
}
