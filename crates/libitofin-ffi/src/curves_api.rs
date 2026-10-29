//! Yield curve construction and queries. All curves retain the same erased handle type.
use crate::boundary::*;
use crate::iterative_bootstrap_api::{CurveAnchor, anchored, iterative_curve};
use crate::time_api::{calendar, date, day_counter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::{
    convexmonotone::ConvexMonotone, cubic::Cubic, flat::BackwardFlat, linear::Linear,
    loglinear::LogLinear,
};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::{
    DiscountCurve, FlatForward, ForwardCurve, InterpolatedDiscountCurve, InterpolatedZeroCurve,
    PiecewiseYieldCurve, ZeroCurve,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::termstructures::{
    bootstraptraits::{Discount, ForwardRate, ZeroYield},
    globalbootstrap::GlobalBootstrap,
    localbootstrap::LocalBootstrap,
};
use libitofin::time::date::Date;
use libitofin::time::frequency::Frequency;

pub(crate) fn curve(c: &Context, id: u64) -> BindingResult<Handle<dyn YieldTermStructure>> {
    c.get(id)
}
pub(crate) fn optional_curve(
    c: &Context,
    id: u64,
) -> BindingResult<Handle<dyn YieldTermStructure>> {
    if id == 0 {
        Ok(Handle::empty())
    } else {
        curve(c, id)
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_flat_forward_new(
    ctx: *mut Context,
    reference: i32,
    rate: f64,
    dc: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = shared(FlatForward::with_rate(
                date(reference)?,
                rate,
                day_counter(c, dc)?,
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>;
            output(out, c.insert(Handle::new(v))?)
        })
    }
}
/// kind: 0 linear zero, 1 cubic zero, 2 log-linear discount, 3 cubic discount, 4 backward-flat forward.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_node_curve_new(
    ctx: *mut Context,
    kind: i32,
    dates: *const i32,
    values: *const f64,
    len: usize,
    dc: u64,
    cal: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let dates = input_slice(dates, len)?
                .iter()
                .map(|v| date(*v))
                .collect::<BindingResult<Vec<_>>>()?;
            let values = input_slice(values, len)?.to_vec();
            let dc = day_counter(c, dc)?;
            let cal = if cal == 0 {
                None
            } else {
                Some(calendar(c, cal)?)
            };
            let v: Shared<dyn YieldTermStructure> = match kind {
                0 => shared(ZeroCurve::new(dates, values, dc, Linear)?),
                1 => shared(InterpolatedZeroCurve::<Cubic>::new(
                    dates, values, dc, Cubic,
                )?),
                2 => shared(DiscountCurve::new(dates, values, dc, cal)?),
                3 => shared(InterpolatedDiscountCurve::<Cubic>::new(
                    dates, values, dc, cal,
                )?),
                4 => shared(ForwardCurve::new(dates, values, dc, BackwardFlat)?),
                _ => return Err(BindingError::invalid("unknown node curve kind")),
            };
            output(out, c.insert(Handle::new(v))?)
        })
    }
}
/// Query: 0 discount(time), 1 discount(date), 2 zero rate, 3 forward rate.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_curve_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    t1: f64,
    t2: f64,
    serial: i32,
    extrapolate: bool,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let v = curve(c, id)?.current_link()?;
            let value = match query {
                0 => v.discount(t1, extrapolate)?,
                1 => v.discount_date(date(serial)?, extrapolate)?,
                2 => v
                    .zero_rate(t1, Compounding::Continuous, Frequency::Annual, extrapolate)?
                    .rate(),
                3 => v
                    .forward_rate(
                        t1,
                        t2,
                        Compounding::Continuous,
                        Frequency::Annual,
                        extrapolate,
                    )?
                    .rate(),
                _ => return Err(BindingError::invalid("unknown curve query")),
            };
            output(out, value)
        })
    }
}
/// Query: 0 reference date, 1 maximum date, 2 extrapolation enabled.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_curve_info(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = curve(c, id)?.current_link()?;
            output(
                out,
                match query {
                    0 => v.reference_date()?.serial_number(),
                    1 => v.max_date().serial_number(),
                    2 => i32::from(v.allows_extrapolation()),
                    _ => return Err(BindingError::invalid("unknown curve info")),
                },
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
pub unsafe extern "C" fn itofin_curve_extrapolation(
    ctx: *mut Context,
    id: u64,
    enabled: bool,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let v = curve(c, id)?.current_link()?;
            if enabled {
                v.enable_extrapolation();
            } else {
                v.disable_extrapolation();
            }
            Ok(())
        })
    }
}
/// kind: discount log-linear/linear/cubic = 0/1/2, zero linear/cubic = 3/4,
/// forward linear/convex-monotone/backward-flat = 5/6/7. Algorithm: iterative/global/local = 0/1/2.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_curve_new(
    ctx: *mut Context,
    reference: i32,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    algorithm: i32,
    additional: *const u64,
    additional_len: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let anchor = CurveAnchor::Fixed(date(reference)?);
            let (h, x) = (
                input_slice(helpers, len)?,
                input_slice(additional, additional_len)?,
            );
            piecewise_curve(c, anchor, h, dc, kind, algorithm, x, out)
        })
    }
}
/// The itofin_piecewise_curve_new twin whose reference date moves with the
/// evaluation date of settings, settlement_days business days on calendar.
/// settings must be the SAME instance the helpers' index carries: a different
/// instance moves the curve's reference date while the helpers stay put.
/// An unset evaluation date succeeds here and errs on the first read.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_curve_new_moving(
    ctx: *mut Context,
    settlement_days: u32,
    calendar: u64,
    settings: u64,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    algorithm: i32,
    additional: *const u64,
    additional_len: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let anchor = CurveAnchor::Moving(
                settlement_days,
                crate::time_api::calendar(c, calendar)?,
                c.get::<Shared<Settings<Date>>>(settings)?,
            );
            let (h, x) = (
                input_slice(helpers, len)?,
                input_slice(additional, additional_len)?,
            );
            piecewise_curve(c, anchor, h, dc, kind, algorithm, x, out)
        })
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn piecewise_curve(
    c: &mut Context,
    anchor: CurveAnchor,
    helpers: &[u64],
    dc: u64,
    kind: i32,
    algorithm: i32,
    additional: &[u64],
    out: *mut u64,
) -> BindingResult<()> {
    unsafe {
        let dc = day_counter(c, dc)?;
        let helpers = helpers
            .iter()
            .map(|id| crate::helpers_api::helper(c, *id))
            .collect::<BindingResult<Vec<_>>>()?;
        let additional = additional
            .iter()
            .map(|id| crate::helpers_api::helper(c, *id))
            .collect::<BindingResult<Vec<_>>>()?;
        if algorithm != 1 && !additional.is_empty() {
            return Err(BindingError::invalid(
                "additional helpers require global bootstrap",
            ));
        }
        let global = || {
            GlobalBootstrap::with_penalties(additional, None, None, None, Vec::new(), |_, _| {
                Vec::new()
            })
        };
        let (a, h) = (anchor, helpers);
        let v = match (kind, algorithm) {
            (0..=7, 0) => iterative_curve(a, h, dc, kind, Default::default())?,
            (6, 2) => anchored::<ForwardRate, _, _>(
                a,
                h,
                dc,
                ConvexMonotone::default(),
                LocalBootstrap::default(),
            )?,
            (0, 1) => anchored::<Discount, _, _>(a, h, dc, LogLinear, global())?,
            (1, 1) => anchored::<Discount, _, _>(a, h, dc, Linear, global())?,
            _ => {
                return Err(BindingError::invalid(
                    "unsupported curve/bootstrap combination",
                ));
            }
        };
        output(out, c.insert(Handle::new(v))?)
    }
}
/// Caller first queries length with capacity=0; then supplies both arrays with that capacity.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_curve_nodes(
    ctx: *mut Context,
    id: u64,
    dates: *mut i32,
    values: *mut f64,
    capacity: usize,
    length: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(length)?;
            let v = curve(c, id)?.current_link()?;
            let any = v
                .as_any()
                .ok_or_else(|| BindingError::invalid("curve has no node introspection"))?;
            macro_rules! nodes { ($($t:ty),+) => {{
            let mut result=None;
            $(if let Some(p)=any.downcast_ref::<$t>() { result=Some((p.dates()?,p.data()?)); })+
            result.ok_or_else(||BindingError::invalid("curve has no piecewise nodes"))?
        }} }
            let (ds, vs) = nodes!(PiecewiseYieldCurve<Discount,LogLinear>,PiecewiseYieldCurve<Discount,Linear>,PiecewiseYieldCurve<Discount,Cubic>,PiecewiseYieldCurve<ZeroYield,Linear>,PiecewiseYieldCurve<ZeroYield,Cubic>,PiecewiseYieldCurve<ForwardRate,Linear>,PiecewiseYieldCurve<ForwardRate,ConvexMonotone>,PiecewiseYieldCurve<ForwardRate,BackwardFlat>,PiecewiseYieldCurve<ForwardRate,ConvexMonotone,LocalBootstrap>,PiecewiseYieldCurve<Discount,LogLinear,GlobalBootstrap>,PiecewiseYieldCurve<Discount,Linear,GlobalBootstrap>);
            output(length, ds.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < ds.len() {
                return Err(BindingError::invalid("node buffer too small"));
            }
            check_ptr(dates)?;
            check_ptr(values)?;
            for (i, (d, v)) in ds.iter().zip(vs.iter()).enumerate() {
                output(dates.add(i), d.serial_number())?;
                output(values.add(i), *v)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar_api::NativeCalendar;
    use libitofin::indexes::Euribor;
    use libitofin::termstructures::RateHelper;
    use libitofin::termstructures::yields::DepositRateHelper;
    use libitofin::time::businessdayconvention::BusinessDayConvention::Following;
    use libitofin::time::calendars::target::Target;
    use libitofin::time::date::Month;
    use libitofin::time::daycounters::actual360::Actual360;
    use libitofin::time::{date::Date, daycounters::actual365fixed::Actual365Fixed};
    use libitofin::time::{period::Period, timeunit::TimeUnit::*};
    use std::ptr::null_mut;
    #[test]
    fn flat_curve_native_boundary_and_lifetimes() {
        let mut c = Context::new();
        let dc = c.insert(Actual365Fixed::new()).unwrap();
        let d = Date::new(15, libitofin::time::date::Month::June, 2026).serial_number();
        let mut id = 0;
        let mut v = 0.0;
        unsafe {
            assert_eq!(
                itofin_flat_forward_new(&mut c, d, 0.04, dc, &mut id, std::ptr::null_mut()),
                0
            );
            assert_eq!(itofin_handle_release(&mut c, dc, std::ptr::null_mut()), 0);
            assert_eq!(
                itofin_curve_value(
                    &mut c,
                    id,
                    0,
                    2.0,
                    0.0,
                    0,
                    false,
                    &mut v,
                    std::ptr::null_mut()
                ),
                0
            );
        }
        assert!((v - (-0.08_f64).exp()).abs() < 1e-14);
        let mut other = Context::new();
        unsafe {
            assert_eq!(
                itofin_curve_value(
                    &mut other,
                    id,
                    0,
                    2.0,
                    0.0,
                    0,
                    false,
                    &mut v,
                    std::ptr::null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(
                itofin_curve_value(
                    &mut c,
                    id,
                    9,
                    0.0,
                    0.0,
                    0,
                    false,
                    &mut v,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_curve_value(
                    &mut c,
                    id,
                    0,
                    2.0,
                    0.0,
                    0,
                    false,
                    std::ptr::null_mut(),
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(itofin_handle_release(&mut c, id, std::ptr::null_mut()), 0);
            assert_eq!(
                itofin_curve_value(
                    &mut c,
                    id,
                    0,
                    2.0,
                    0.0,
                    0,
                    false,
                    &mut v,
                    std::ptr::null_mut()
                ),
                INVALID_HANDLE
            );
        }
    }
    #[test]
    fn node_curve_checks_dates_and_extrapolation() {
        let mut c = Context::new();
        let dc = c.insert(Actual365Fixed::new()).unwrap();
        let dates = [
            Date::new(15, libitofin::time::date::Month::June, 2026).serial_number(),
            Date::new(15, libitofin::time::date::Month::June, 2027).serial_number(),
        ];
        let values = [1.0, 0.95];
        let mut id = 0;
        let mut v = 0.0;
        unsafe {
            assert_eq!(
                itofin_node_curve_new(
                    &mut c,
                    2,
                    dates.as_ptr(),
                    values.as_ptr(),
                    2,
                    dc,
                    0,
                    &mut id,
                    std::ptr::null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_curve_value(
                    &mut c,
                    id,
                    0,
                    1.0,
                    0.0,
                    0,
                    false,
                    &mut v,
                    std::ptr::null_mut()
                ),
                0
            );
            assert!((v - 0.95).abs() < 1e-14);
            assert_ne!(
                itofin_curve_value(
                    &mut c,
                    id,
                    0,
                    2.0,
                    0.0,
                    0,
                    false,
                    &mut v,
                    std::ptr::null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_curve_extrapolation(&mut c, id, true, std::ptr::null_mut()),
                0
            );
            assert_eq!(
                itofin_curve_value(
                    &mut c,
                    id,
                    0,
                    2.0,
                    0.0,
                    0,
                    false,
                    &mut v,
                    std::ptr::null_mut()
                ),
                0
            );
            assert!((v - 0.95_f64.powi(2)).abs() < 1e-14);
            assert_eq!(
                itofin_node_curve_new(
                    &mut c,
                    2,
                    std::ptr::null(),
                    values.as_ptr(),
                    2,
                    dc,
                    0,
                    &mut id,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
        }
    }

    fn deposits(c: &mut Context, settings: &Shared<Settings<Date>>) -> Vec<u64> {
        let mut ids = Vec::new();
        for (unit, rate) in [(Weeks, 0.04559), (Months, 0.04581), (Years, 0.04496)] {
            let period = Period::new(1, unit);
            let index = Euribor::new(period, Handle::empty(), Shared::clone(settings));
            let helper = DepositRateHelper::from_rate(rate, &index.unwrap());
            ids.push(c.insert(helper as Shared<dyn RateHelper>).unwrap());
        }
        ids
    }

    unsafe fn moving(c: &mut Context, ids: [u64; 3], helpers: &[u64], kind: [i32; 2]) -> u64 {
        let [cal, settings, dc] = ids;
        let (h, mut id) = (helpers, 0);
        let (k, a, null) = (kind[0], kind[1], std::ptr::null());
        let rc = unsafe {
            itofin_piecewise_curve_new_moving(
                c,
                2,
                cal,
                settings,
                h.as_ptr(),
                h.len(),
                dc,
                k,
                a,
                null,
                0,
                &mut id,
                null_mut(),
            )
        };
        assert_eq!(rc, 0);
        id
    }

    fn nodes(c: &mut Context, id: u64) -> (Vec<i32>, Vec<f64>) {
        let (mut dates, mut values, mut n) = (vec![0; 8], vec![0.0; 8], 0);
        let (d, v) = (dates.as_mut_ptr(), values.as_mut_ptr());
        let rc = unsafe { itofin_curve_nodes(c, id, d, v, 8, &mut n, null_mut()) };
        assert_eq!(rc, 0);
        (dates[..n].to_vec(), values[..n].to_vec())
    }

    /// The moving curve and its helpers share one Settings (D5): a
    /// 15-business-day roll moves the reference date, re-bootstraps, and
    /// matches a fixed curve built at the new reference date within 1e-10.
    /// An unset evaluation date builds and errs on the first read (D10).
    #[test]
    fn moving_piecewise_curve_follows_a_shared_settings_roll() {
        let advance = |d, n| Target::new().advance(d, n, Days, Following, false);
        for kind in [[0, 0], [6, 2], [0, 1]] {
            let mut c = Context::new();
            let today = Date::new(15, Month::June, 2026);
            let settings = shared(Settings::<Date>::new());
            settings.set_evaluation_date(today);
            let s = c.insert(Shared::clone(&settings)).unwrap();
            let unset = c.insert(shared(Settings::<Date>::new())).unwrap();
            let dc = c.insert(Actual360::new()).unwrap();
            let calendar = NativeCalendar {
                inner: Target::new(),
                horizon: None,
                first_year: 1901,
            };
            let cal = c.insert(calendar).unwrap();
            let helpers = deposits(&mut c, &settings);
            let (mut fixed, mut serial) = (0, 0);
            unsafe {
                let id = moving(&mut c, [cal, s, dc], &helpers, kind);
                let orphan = moving(&mut c, [cal, unset, dc], &helpers, kind);
                let info = itofin_curve_info(&mut c, orphan, 0, &mut serial, null_mut());
                assert_eq!(info, CORE_ERROR);
                let (before, _) = nodes(&mut c, id);
                let new_today = advance(today, 15);
                settings.set_evaluation_date(new_today);
                let info = itofin_curve_info(&mut c, id, 0, &mut serial, null_mut());
                assert_eq!(info, 0);
                assert_eq!(serial, advance(new_today, 2).serial_number());
                let (rolled, values) = nodes(&mut c, id);
                assert_ne!(rolled, before);
                let h = deposits(&mut c, &settings);
                let (k, a, null) = (kind[0], kind[1], std::ptr::null());
                let rc = itofin_piecewise_curve_new(
                    &mut c,
                    serial,
                    h.as_ptr(),
                    3,
                    dc,
                    k,
                    a,
                    null,
                    0,
                    &mut fixed,
                    null_mut(),
                );
                assert_eq!(rc, 0);
                let (dates, expected) = nodes(&mut c, fixed);
                assert_eq!(rolled, dates);
                for (v, e) in values.iter().zip(&expected) {
                    assert!((v - e).abs() <= 1e-10, "{v} vs {e}");
                }
            }
        }
    }
}
