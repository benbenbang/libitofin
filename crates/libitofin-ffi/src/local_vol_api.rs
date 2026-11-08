//! Local volatility term structures; handles retain the common trait representation.
//!
//! Deferred: only `LocalVolCurve<Linear>` is bound, the one variance curve
//! interpolation the C ABI builds (#624); FD local-vol pricing is #828.
use crate::boundary::*;
use libitofin::handle::Handle;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    BlackVolTermStructure, LocalVolCurve, LocalVolSurface, LocalVolTermStructure,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;

pub(crate) type LocalVol = Handle<dyn LocalVolTermStructure>;

/// Dupire local volatility over a Black vol handle, risk-free and dividend
/// curve handles and the session quote `spot`, which the surface observes.
/// # Safety
/// Follow the crate C caller contract; outputs must be live and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_local_vol_surface_new(
    ctx: *mut Context,
    black_vol: u64,
    risk_free: u64,
    dividend: u64,
    spot: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = shared(LocalVolSurface::new(
                c.get::<Handle<dyn BlackVolTermStructure>>(black_vol)?,
                c.get::<Handle<dyn YieldTermStructure>>(risk_free)?,
                c.get::<Handle<dyn YieldTermStructure>>(dividend)?,
                Handle::new(c.get::<Shared<SimpleQuote>>(spot)? as Shared<dyn Quote>),
            )) as Shared<dyn LocalVolTermStructure>;
            output(out, c.insert(Handle::new(v))?)
        })
    }
}

/// Local volatility of a linear Black variance curve built from the same
/// arguments as `itofin_black_variance_curve_new` (extrapolation: 0 flat
/// volatility, 1 use interpolator, 2 linear variance).
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_local_vol_curve_new(
    ctx: *mut Context,
    reference_date: i32,
    dates: *const i32,
    vols: *const f64,
    count: usize,
    dc: u64,
    monotone: i32,
    extrapolation: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let curve = crate::vol_api::variance_curve(
                c,
                reference_date,
                dates,
                vols,
                count,
                dc,
                monotone,
                extrapolation,
            )?;
            let v =
                shared(LocalVolCurve::new(Handle::new(curve))) as Shared<dyn LocalVolTermStructure>;
            output(out, c.insert(Handle::new(v))?)
        })
    }
}

/// Local volatility at time `t` (a year fraction) and underlying level `strike`.
/// # Safety
/// Follow the crate C caller contract; outputs must be live and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_local_vol_query(
    ctx: *mut Context,
    id: u64,
    t: f64,
    strike: f64,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !t.is_finite() || !strike.is_finite() {
                return Err(BindingError::invalid("query inputs must be finite"));
            }
            let extrapolate = match extrapolate {
                0 => false,
                1 => true,
                _ => return Err(BindingError::invalid("expected boolean 0 or 1")),
            };
            let v = c.get::<LocalVol>(id)?.current_link()?;
            output(out, v.local_vol(t, strike, extrapolate)?)
        })
    }
}
