//! Credit curves preserve the live quote and bootstrap observer graph.
use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::credit::{
    defaultprobabilityhelpers::DefaultProbabilityHelper,
    defaulttermstructure::DefaultProbabilityTermStructure, flathazardrate::FlatHazardRate,
    interpolatedhazardratecurve::InterpolatedHazardRateCurve,
    piecewisedefaultcurve::PiecewiseDefaultCurve, probabilitytraits::HazardRate,
};
use libitofin::time::{calendar::Calendar, date::Date};

type Interpolated = InterpolatedHazardRateCurve<BackwardFlat>;
type Piecewise = PiecewiseDefaultCurve<HazardRate, BackwardFlat>;
#[derive(Clone)]
pub(crate) struct CreditCurve {
    pub handle: Handle<dyn DefaultProbabilityTermStructure>,
    interpolated: Option<Shared<Interpolated>>,
    piecewise: Option<Shared<Piecewise>>,
}
impl CreditCurve {
    fn flat(curve: FlatHazardRate) -> Self {
        Self {
            handle: Handle::new(shared(curve)),
            interpolated: None,
            piecewise: None,
        }
    }
}
/// A zero quote handle selects `rate`; a nonzero settings handle selects moving dates.
#[repr(C)]
pub struct ItofinFlatHazardConfig {
    pub reference_date: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub quote: u64,
    pub rate: f64,
    pub day_counter: u64,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_flat_hazard_new(
    ctx: *mut Context,
    config: *const ItofinFlatHazardConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            let dc = day_counter(c, a.day_counter)?;
            let q: Handle<dyn Quote> = if a.quote == 0 {
                if !a.rate.is_finite() {
                    return Err(BindingError::invalid("nonfinite hazard rate"));
                }
                Handle::new(shared(SimpleQuote::new(a.rate)))
            } else {
                Handle::new(c.get::<Shared<SimpleQuote>>(a.quote)?)
            };
            let curve = if a.settings == 0 {
                FlatHazardRate::new(date(a.reference_date)?, q, dc)
            } else {
                FlatHazardRate::moving(
                    a.settlement_days,
                    c.get::<Calendar>(a.calendar)?,
                    q,
                    dc,
                    c.get::<Shared<Settings<Date>>>(a.settings)?,
                )
            };
            output(out, c.insert(CreditCurve::flat(curve))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_interpolated_hazard_new(
    ctx: *mut Context,
    dates: *const i32,
    rates: *const f64,
    count: usize,
    dc: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let dates = input_slice(dates, count)?
                .iter()
                .map(|v| date(*v))
                .collect::<BindingResult<Vec<_>>>()?;
            let rates = input_slice(rates, count)?.to_vec();
            if rates.iter().any(|v| !v.is_finite()) {
                return Err(BindingError::invalid("nonfinite hazard rate"));
            }
            let concrete = shared(Interpolated::new(
                dates,
                rates,
                day_counter(c, dc)?,
                BackwardFlat,
            )?);
            let curve = CreditCurve {
                handle: Handle::new(concrete.clone()),
                interpolated: Some(concrete),
                piecewise: None,
            };
            output(out, c.insert(curve)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_default_new(
    ctx: *mut Context,
    reference: i32,
    helpers: *const u64,
    count: usize,
    dc: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let helpers = input_slice(helpers, count)?
                .iter()
                .map(|id| c.get::<Shared<dyn DefaultProbabilityHelper>>(*id))
                .collect::<BindingResult<Vec<_>>>()?;
            let concrete =
                Piecewise::new(date(reference)?, helpers, day_counter(c, dc)?, BackwardFlat)?;
            let curve = CreditCurve {
                handle: Handle::new(concrete.clone()),
                interpolated: None,
                piecewise: Some(concrete),
            };
            output(out, c.insert(curve)?)
        })
    }
}
/// kind: 0 survival, 1 default probability, 2 density, 3 hazard. use_date: 0 time, 1 date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_default_curve_value(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    t: f64,
    serial: i32,
    use_date: i32,
    extrapolate: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !matches!(extrapolate, 0 | 1) || !matches!(use_date, 0 | 1) {
                return Err(BindingError::invalid("boolean must be 0 or 1"));
            }
            let curve = c.get::<CreditCurve>(id)?.handle.current_link()?;
            let ex = extrapolate != 0;
            let value = if use_date == 1 {
                let d = date(serial)?;
                match kind {
                    0 => curve.survival_probability_date(d, ex)?,
                    1 => curve.default_probability_date(d, ex)?,
                    2 => curve.default_density_date(d, ex)?,
                    3 => curve.hazard_rate_date(d, ex)?,
                    _ => return Err(BindingError::invalid("unknown credit query")),
                }
            } else {
                if !t.is_finite() {
                    return Err(BindingError::invalid("nonfinite time"));
                }
                match kind {
                    0 => curve.survival_probability(t, ex)?,
                    1 => curve.default_probability(t, ex)?,
                    2 => curve.default_density(t, ex)?,
                    3 => curve.hazard_rate(t, ex)?,
                    _ => return Err(BindingError::invalid("unknown credit query")),
                }
            };
            output(out, value)
        })
    }
}
#[repr(C)]
pub struct ItofinCreditNode {
    pub date: i32,
    pub time: f64,
    pub rate: f64,
}
/// capacity zero queries the required length. Node reads run the lazy bootstrap.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_default_curve_nodes(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinCreditNode,
    capacity: usize,
    count: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(count)?;
            let curve = c.get::<CreditCurve>(id)?;
            let (dates, times, rates) = if let Some(p) = curve.piecewise {
                (p.dates()?, p.times()?, p.data()?)
            } else if let Some(p) = curve.interpolated {
                (
                    p.dates().to_vec(),
                    p.times().to_vec(),
                    p.hazard_rates().to_vec(),
                )
            } else {
                return Err(BindingError::invalid("flat curve has no nodes"));
            };
            output(count, dates.len())?;
            if capacity == 0 {
                return Ok(());
            }
            check_ptr(out)?;
            if capacity < dates.len() {
                return Err(BindingError::invalid("node buffer too small"));
            }
            for (i, ((d, t), r)) in dates.into_iter().zip(times).zip(rates).enumerate() {
                output(
                    out.add(i),
                    ItofinCreditNode {
                        date: d.serial_number(),
                        time: t,
                        rate: r,
                    },
                )?;
            }
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_default_curve_calculate(
    ctx: *mut Context,
    id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<CreditCurve>(id)?
                .piecewise
                .ok_or_else(|| BindingError::invalid("not a piecewise default curve"))?
                .calculate()?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::time::date::Month;
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    #[test]
    fn flat_hazard_live_quote_lifetime_and_foreign_handle() {
        let mut c = Context::new();
        let dc = c.insert(Actual365Fixed::new()).unwrap();
        let quote = shared(SimpleQuote::new(0.01234));
        let q = c.insert(quote.clone()).unwrap();
        let cfg = ItofinFlatHazardConfig {
            reference_date: Date::new(9, Month::June, 2006).serial_number(),
            settlement_days: 0,
            calendar: 0,
            quote: q,
            rate: 0.0,
            day_counter: dc,
            settings: 0,
        };
        let mut id = 0;
        let mut v = 0.0;
        unsafe {
            assert_eq!(
                itofin_flat_hazard_new(&mut c, &cfg, &mut id, std::ptr::null_mut()),
                0
            );
            assert_eq!(
                itofin_default_curve_value(
                    &mut c,
                    id,
                    0,
                    10.0,
                    0,
                    0,
                    0,
                    &mut v,
                    std::ptr::null_mut()
                ),
                0
            );
            assert!((v - (-0.1234_f64).exp()).abs() < 1e-14);
            quote.set_value(0.02);
            assert_eq!(itofin_handle_release(&mut c, q, std::ptr::null_mut()), 0);
            assert_eq!(
                itofin_default_curve_value(
                    &mut c,
                    id,
                    0,
                    10.0,
                    0,
                    0,
                    0,
                    &mut v,
                    std::ptr::null_mut()
                ),
                0
            );
            assert!((v - (-0.2_f64).exp()).abs() < 1e-14);
            let mut other = Context::new();
            assert_eq!(
                itofin_default_curve_value(
                    &mut other,
                    id,
                    0,
                    1.0,
                    0,
                    0,
                    0,
                    &mut v,
                    std::ptr::null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(
                itofin_default_curve_value(
                    &mut c,
                    id,
                    0,
                    f64::NAN,
                    0,
                    0,
                    0,
                    &mut v,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_default_curve_value(
                    &mut c,
                    id,
                    0,
                    1.0,
                    0,
                    2,
                    0,
                    &mut v,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(itofin_handle_release(&mut c, id, std::ptr::null_mut()), 0);
            assert_eq!(
                itofin_default_curve_value(
                    &mut c,
                    id,
                    0,
                    1.0,
                    0,
                    0,
                    0,
                    &mut v,
                    std::ptr::null_mut()
                ),
                INVALID_HANDLE
            );
        }
    }
}
