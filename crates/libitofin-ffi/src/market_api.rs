//! Observable quotes and Black–Scholes market inputs.
use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;

pub(crate) fn quote(c: &Context, id: u64) -> BindingResult<Handle<dyn Quote>> {
    Ok(Handle::new(c.get::<Shared<SimpleQuote>>(id)? as Shared<dyn Quote>))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_quote_new(ctx: *mut Context, value: f64, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| { check_ptr(out)?; output(out, c.insert(shared(SimpleQuote::new(value)))?) }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_quote_value(ctx: *mut Context, id: u64, out: *mut f64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| { output(out, c.get::<Shared<SimpleQuote>>(id)?.value()?) }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_quote_set(ctx: *mut Context, id: u64, value: f64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| { c.get::<Shared<SimpleQuote>>(id)?.set_value(value); Ok(()) }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_scholes_new(ctx: *mut Context, spot: f64, risk_free: f64, dividend: f64, volatility: f64, reference: i32, dc: u64, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let reference = date(reference)?;
        let dc = day_counter(c, dc)?;
        let flat = |rate| Handle::new(shared(FlatForward::with_rate(reference, rate, dc.clone(), Compounding::Continuous, Frequency::Annual)) as Shared<dyn YieldTermStructure>);
        let process = shared(GeneralizedBlackScholesProcess::new(
            Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>),
            flat(dividend), flat(risk_free),
            Handle::new(shared(BlackConstantVol::new(reference, None, volatility, dc)) as Shared<dyn BlackVolTermStructure>),
        ));
        output(out, c.insert(process)?)
    }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_scholes_from_curves(ctx: *mut Context, spot: f64, risk_free: u64, dividend: u64, volatility: u64, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let process = shared(GeneralizedBlackScholesProcess::new(
            Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>),
            c.get::<Handle<dyn YieldTermStructure>>(dividend)?,
            c.get::<Handle<dyn YieldTermStructure>>(risk_free)?,
            c.get::<Handle<dyn BlackVolTermStructure>>(volatility)?,
        ));
        output(out, c.insert(process)?)
    }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_scholes_rate(ctx: *mut Context, id: u64, dividend: bool, out: *mut f64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        let process = c.get::<Shared<GeneralizedBlackScholesProcess>>(id)?;
        let curve = if dividend { process.dividend_yield() } else { process.risk_free_rate() };
        output(out, curve.current_link()?.zero_rate(0.0, Compounding::Continuous, Frequency::Annual, true)?.rate())
    }) }
}
