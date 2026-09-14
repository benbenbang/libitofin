//! Multiplicative price seasonality factors.
use crate::boundary::*;
use crate::time_api::{date,frequency};
use crate::inflation_curves_api::frequency_code;
use libitofin::shared::{Shared,shared};
use libitofin::termstructures::inflation::seasonality::MultiplicativePriceSeasonality;
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_seasonality_new(ctx:*mut Context,base:i32,freq:i32,factors:*const f64,n:usize,out:*mut u64,error:*mut ItofinError)->i32{
 unsafe{with_context(ctx,error,|c|{check_ptr(out)?;let values=input_slice(factors,n)?.to_vec();
 if values.iter().any(|v|!v.is_finite()){return Err(BindingError::invalid("nonfinite seasonality factor"))}
 let p=shared(MultiplicativePriceSeasonality::new(date(base)?,frequency(freq)?,values)?);output(out,c.insert(p)?)})}
}
/// query 0 base date serial, 1 frequency, 2 factor at date.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_seasonality_value(ctx:*mut Context,id:u64,query:i32,serial:i32,out:*mut f64,error:*mut ItofinError)->i32{
 unsafe{with_context(ctx,error,|c|{check_ptr(out)?;let p=c.get::<Shared<MultiplicativePriceSeasonality>>(id)?;
 output(out,match query{0=>p.seasonality_base_date().serial_number() as f64,1=>frequency_code(p.frequency())? as f64,2=>p.seasonality_factor(date(serial)?)?,_=>return Err(BindingError::invalid("unknown seasonality query"))})})}
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_seasonality_factors(ctx:*mut Context,id:u64,out:*mut f64,capacity:usize,count:*mut usize,error:*mut ItofinError)->i32{
 unsafe{with_context(ctx,error,|c|{check_ptr(count)?;let p=c.get::<Shared<MultiplicativePriceSeasonality>>(id)?;let values=p.seasonality_factors();
 output(count,values.len())?;if capacity==0{return Ok(())}check_ptr(out)?;if capacity<values.len(){return Err(BindingError::invalid("factor buffer too small"))}
 for(i,v)in values.iter().enumerate(){output(out.add(i),*v)?;}Ok(())})}
}
