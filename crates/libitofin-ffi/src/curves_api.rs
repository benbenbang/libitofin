//! Yield curve construction and queries. All curves retain the same erased handle type.
use crate::boundary::*;
use crate::time_api::{date, day_counter, calendar};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::{convexmonotone::ConvexMonotone, cubic::Cubic, flat::BackwardFlat, linear::Linear, loglinear::LogLinear};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::{RateHelper, bootstraptraits::{Discount, ForwardRate, ZeroYield}, globalbootstrap::GlobalBootstrap, localbootstrap::LocalBootstrap};
use libitofin::termstructures::yields::{FlatForward, ZeroCurve, DiscountCurve, ForwardCurve, InterpolatedDiscountCurve, InterpolatedZeroCurve, PiecewiseYieldCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;

pub(crate) fn curve(c: &Context, id: u64) -> BindingResult<Handle<dyn YieldTermStructure>> { c.get(id) }
pub(crate) fn optional_curve(c: &Context, id: u64) -> BindingResult<Handle<dyn YieldTermStructure>> {
    if id == 0 { Ok(Handle::empty()) } else { curve(c, id) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_flat_forward_new(ctx: *mut Context, reference: i32, rate: f64, dc: u64, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx,error,|c| {
        check_ptr(out)?;
        let v = shared(FlatForward::with_rate(date(reference)?,rate,day_counter(c,dc)?,Compounding::Continuous,Frequency::Annual)) as Shared<dyn YieldTermStructure>;
        output(out,c.insert(Handle::new(v))?)
    }) }
}
/// kind: 0 linear zero, 1 cubic zero, 2 log-linear discount, 3 cubic discount, 4 backward-flat forward.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_node_curve_new(ctx: *mut Context, kind: i32, dates: *const i32, values: *const f64, len: usize, dc: u64, cal: u64, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx,error,|c| {
        check_ptr(out)?;
        let dates = input_slice(dates,len)?.iter().map(|v| date(*v)).collect::<BindingResult<Vec<_>>>()?;
        let values = input_slice(values,len)?.to_vec();
        let dc = day_counter(c,dc)?;
        let cal = if cal==0 { None } else { Some(calendar(c,cal)?) };
        let v: Shared<dyn YieldTermStructure> = match kind {
            0 => shared(ZeroCurve::new(dates,values,dc,Linear)?),
            1 => shared(InterpolatedZeroCurve::<Cubic>::new(dates,values,dc,Cubic)?),
            2 => shared(DiscountCurve::new(dates,values,dc,cal)?),
            3 => shared(InterpolatedDiscountCurve::<Cubic>::new(dates,values,dc,cal)?),
            4 => shared(ForwardCurve::new(dates,values,dc,BackwardFlat)?),
            _ => return Err(BindingError::invalid("unknown node curve kind")),
        };
        output(out,c.insert(Handle::new(v))?)
    }) }
}
/// Query: 0 discount(time), 1 discount(date), 2 zero rate, 3 forward rate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_curve_value(ctx: *mut Context, id: u64, query: i32, t1: f64, t2: f64, serial: i32, extrapolate: bool, out: *mut f64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx,error,|c| {
        check_ptr(out)?;
        let v = curve(c,id)?.current_link()?;
        let value = match query {
            0 => v.discount(t1,extrapolate)?,
            1 => v.discount_date(date(serial)?,extrapolate)?,
            2 => v.zero_rate(t1,Compounding::Continuous,Frequency::Annual,extrapolate)?.rate(),
            3 => v.forward_rate(t1,t2,Compounding::Continuous,Frequency::Annual,extrapolate)?.rate(),
            _ => return Err(BindingError::invalid("unknown curve query")),
        };
        output(out,value)
    }) }
}
/// Query: 0 reference date, 1 maximum date, 2 extrapolation enabled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_curve_info(ctx: *mut Context, id: u64, query: i32, out: *mut i32, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx,error,|c| {
        let v = curve(c,id)?.current_link()?;
        output(out,match query {
            0 => v.reference_date()?.serial_number(), 1 => v.max_date().serial_number(),
            2 => i32::from(v.allows_extrapolation()),
            _ => return Err(BindingError::invalid("unknown curve info")),
        })
    }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_curve_extrapolation(ctx: *mut Context, id: u64, enabled: bool, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx,error,|c| {
        let v = curve(c,id)?.current_link()?;
        if enabled { v.enable_extrapolation(); } else { v.disable_extrapolation(); } Ok(())
    }) }
}
/// kind: discount log-linear/linear/cubic = 0/1/2, zero linear/cubic = 3/4,
/// forward linear/convex-monotone/backward-flat = 5/6/7. Algorithm: iterative/global/local = 0/1/2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_piecewise_curve_new(ctx: *mut Context, reference: i32, helpers: *const u64, len: usize, dc: u64, kind: i32, algorithm: i32, additional: *const u64, additional_len: usize, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx,error,|c| {
        check_ptr(out)?;
        let reference=date(reference)?; let dc=day_counter(c,dc)?;
        let helpers = input_slice(helpers,len)?.iter().map(|id| crate::helpers_api::helper(c,*id)).collect::<BindingResult<Vec<_>>>()?;
        let additional = input_slice(additional,additional_len)?.iter().map(|id| crate::helpers_api::helper(c,*id)).collect::<BindingResult<Vec<_>>>()?;
        if algorithm != 1 && !additional.is_empty() { return Err(BindingError::invalid("additional helpers require global bootstrap")); }
        let v: Shared<dyn YieldTermStructure> = match (kind,algorithm) {
            (0,0) => PiecewiseYieldCurve::<Discount,LogLinear>::new(reference,helpers,dc,LogLinear)?,
            (1,0) => PiecewiseYieldCurve::<Discount,Linear>::new(reference,helpers,dc,Linear)?,
            (2,0) => PiecewiseYieldCurve::<Discount,Cubic>::new(reference,helpers,dc,Cubic)?,
            (3,0) => PiecewiseYieldCurve::<ZeroYield,Linear>::new(reference,helpers,dc,Linear)?,
            (4,0) => PiecewiseYieldCurve::<ZeroYield,Cubic>::new(reference,helpers,dc,Cubic)?,
            (5,0) => PiecewiseYieldCurve::<ForwardRate,Linear>::new(reference,helpers,dc,Linear)?,
            (6,0) => PiecewiseYieldCurve::<ForwardRate,ConvexMonotone>::new(reference,helpers,dc,ConvexMonotone::default())?,
            (7,0) => PiecewiseYieldCurve::<ForwardRate,BackwardFlat>::new(reference,helpers,dc,BackwardFlat)?,
            (6,2) => PiecewiseYieldCurve::<ForwardRate,ConvexMonotone,LocalBootstrap>::new(reference,helpers,dc,ConvexMonotone::default())?,
            (0,1) => PiecewiseYieldCurve::<Discount,LogLinear,GlobalBootstrap>::with_bootstrap(reference,helpers,dc,LogLinear,GlobalBootstrap::with_penalties(additional,None,None,None,Vec::new(),|_,_| Vec::new()))?,
            (1,1) => PiecewiseYieldCurve::<Discount,Linear,GlobalBootstrap>::with_bootstrap(reference,helpers,dc,Linear,GlobalBootstrap::with_penalties(additional,None,None,None,Vec::new(),|_,_| Vec::new()))?,
            _ => return Err(BindingError::invalid("unsupported curve/bootstrap combination")),
        };
        output(out,c.insert(Handle::new(v))?)
    }) }
}
/// Caller first queries length with capacity=0; then supplies both arrays with that capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_curve_nodes(ctx: *mut Context, id: u64, dates: *mut i32, values: *mut f64, capacity: usize, length: *mut usize, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx,error,|c| {
        check_ptr(length)?;
        let v=curve(c,id)?.current_link()?;
        let any=v.as_any().ok_or_else(||BindingError::invalid("curve has no node introspection"))?;
        macro_rules! nodes { ($($t:ty),+) => {{
            let mut result=None;
            $(if let Some(p)=any.downcast_ref::<$t>() { result=Some((p.dates()?,p.data()?)); })+
            result.ok_or_else(||BindingError::invalid("curve has no piecewise nodes"))?
        }} }
        let (ds,vs)=nodes!(PiecewiseYieldCurve<Discount,LogLinear>,PiecewiseYieldCurve<Discount,Linear>,PiecewiseYieldCurve<Discount,Cubic>,PiecewiseYieldCurve<ZeroYield,Linear>,PiecewiseYieldCurve<ZeroYield,Cubic>,PiecewiseYieldCurve<ForwardRate,Linear>,PiecewiseYieldCurve<ForwardRate,ConvexMonotone>,PiecewiseYieldCurve<ForwardRate,BackwardFlat>,PiecewiseYieldCurve<ForwardRate,ConvexMonotone,LocalBootstrap>,PiecewiseYieldCurve<Discount,LogLinear,GlobalBootstrap>,PiecewiseYieldCurve<Discount,Linear,GlobalBootstrap>);
        output(length,ds.len())?;
        if capacity==0 { return Ok(()); }
        if capacity<ds.len() { return Err(BindingError::invalid("node buffer too small")); }
        check_ptr(dates)?; check_ptr(values)?;
        for (i,(d,v)) in ds.iter().zip(vs.iter()).enumerate() { output(dates.add(i),d.serial_number())?; output(values.add(i),*v)?; }
        Ok(())
    }) }
}
