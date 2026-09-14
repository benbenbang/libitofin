//! Forward rate agreements retain index and discount handles independently.
use crate::boundary::*;
use crate::rates_api::{curve,finite};
use crate::time_api::date;
use libitofin::handle::Handle;
use libitofin::indexes::{IborIndex,InterestRateIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::ForwardRateAgreement;
use libitofin::position::Position;
use libitofin::shared::{Shared,SharedMut,shared_mut};
use libitofin::time::date::Date;
use libitofin::types::Real;

pub(crate) struct Fra { inner:ForwardRateAgreement, value:Date, maturity:Date }
#[repr(C)]
pub struct ItofinFraConfig {
    pub index:u64,pub value_date:i32,/// Zero derives the index maturity.
    pub maturity_date:i32,pub position:i32,pub strike:Real,pub notional:Real,pub discount:u64,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fra_new(ctx:*mut Context,a:ItofinFraConfig,out:*mut u64,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;
        let index=c.get::<Shared<IborIndex>>(a.index)?;
        let value=date(a.value_date)?;
        let pos=match a.position {0=>Position::Long,1=>Position::Short,_=>return Err(BindingError::invalid("invalid position"))};
        let discount=if a.discount==0 {Handle::empty()}else{curve(c,a.discount)?};
        let maturity=if a.maturity_date==0 {index.maturity_date(value)?}else{date(a.maturity_date)?};
        let inner=if a.maturity_date==0 {
            ForwardRateAgreement::new(index,value,pos,finite(a.strike)?,finite(a.notional)?,discount)?
        }else{ForwardRateAgreement::with_maturity(index,value,maturity,pos,finite(a.strike)?,finite(a.notional)?,discount)?};
        let maturity=inner.calendar().adjust(maturity,inner.business_day_convention());
        output(out,c.insert(shared_mut(Fra{inner,value,maturity}))?)
    })}
}
/// Field 0 NPV, 1 forward rate, 2 amount.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fra_value(ctx:*mut Context,id:u64,field:i32,out:*mut Real,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        check_ptr(out)?;let obj=c.get::<SharedMut<Fra>>(id)?;let mut a=obj.borrow_mut();
        let v=match field {0=>a.inner.npv()?,1=>a.inner.forward_rate()?.rate(),2=>a.inner.amount()?,_=>return Err(BindingError::invalid("invalid FRA field"))};output(out,v)
    })}
}
/// Field 0 value date, 1 adjusted maturity date.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fra_date(ctx:*mut Context,id:u64,field:i32,out:*mut i32,error:*mut ItofinError)->i32 {
    unsafe {with_context(ctx,error,|c| {
        let obj=c.get::<SharedMut<Fra>>(id)?;let a=obj.borrow();
        let d=match field {0=>a.value,1=>a.maturity,_=>return Err(BindingError::invalid("invalid FRA date field"))};output(out,d.serial_number())
    })}
}
