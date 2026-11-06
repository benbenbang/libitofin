//! Credit curves preserve the live quote and bootstrap observer graph.
use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::math::interpolations::linear::Linear;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::credit::{
    defaultprobabilityhelpers::DefaultProbabilityHelper,
    defaulttermstructure::DefaultProbabilityTermStructure,
    flathazardrate::FlatHazardRate,
    interpolateddefaultdensitycurve::InterpolatedDefaultDensityCurve,
    interpolatedhazardratecurve::InterpolatedHazardRateCurve,
    piecewisedefaultcurve::PiecewiseDefaultCurve,
    probabilitytraits::{DefaultDensity, HazardRate},
};
use libitofin::time::date::Date;

type Interpolated = InterpolatedHazardRateCurve<BackwardFlat>;
type Piecewise = PiecewiseDefaultCurve<HazardRate, BackwardFlat>;
#[derive(Clone)]
enum DensityCurve {
    Backward(Shared<InterpolatedDefaultDensityCurve<BackwardFlat>>),
    Linear(Shared<InterpolatedDefaultDensityCurve<Linear>>),
    PiecewiseBackward(Shared<PiecewiseDefaultCurve<DefaultDensity, BackwardFlat>>),
    PiecewiseLinear(Shared<PiecewiseDefaultCurve<DefaultDensity, Linear>>),
}
impl DensityCurve {
    fn nodes(&self) -> BindingResult<(Vec<Date>, Vec<f64>, Vec<f64>)> {
        Ok(match self {
            Self::Backward(p) => (p.dates().to_vec(), p.times().to_vec(), p.data().to_vec()),
            Self::Linear(p) => (p.dates().to_vec(), p.times().to_vec(), p.data().to_vec()),
            Self::PiecewiseBackward(p) => (p.dates()?, p.times()?, p.data()?),
            Self::PiecewiseLinear(p) => (p.dates()?, p.times()?, p.data()?),
        })
    }
    fn calculate(&self) -> BindingResult<()> {
        match self {
            Self::PiecewiseBackward(p) => p.calculate()?,
            Self::PiecewiseLinear(p) => p.calculate()?,
            _ => return Err(BindingError::invalid("not a piecewise default curve")),
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct CreditCurve {
    pub handle: Handle<dyn DefaultProbabilityTermStructure>,
    interpolated: Option<Shared<Interpolated>>,
    piecewise: Option<Shared<Piecewise>>,
    density: Option<DensityCurve>,
}
impl CreditCurve {
    fn flat(curve: FlatHazardRate) -> Self {
        Self {
            handle: Handle::new(shared(curve)),
            interpolated: None,
            piecewise: None,
            density: None,
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
        itofin_flat_hazard_with_jumps_new(
            ctx,
            config,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            out,
            error,
        )
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_flat_hazard_with_jumps_new(
    ctx: *mut Context,
    config: *const ItofinFlatHazardConfig,
    jumps: *const u64,
    jump_count: usize,
    dates: *const i32,
    date_count: usize,
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
            let jumps = input_slice(jumps, jump_count)?
                .iter()
                .map(|id| {
                    c.get::<Shared<SimpleQuote>>(*id)
                        .map(|q| Handle::new(q as Shared<dyn Quote>))
                })
                .collect::<BindingResult<Vec<_>>>()?;
            let dates = input_slice(dates, date_count)?
                .iter()
                .map(|d| date(*d))
                .collect::<BindingResult<Vec<_>>>()?;
            let curve = if a.settings == 0 {
                FlatHazardRate::with_jumps(date(a.reference_date)?, q, dc, jumps, dates)?
            } else {
                FlatHazardRate::moving_with_jumps(
                    a.settlement_days,
                    crate::time_api::calendar(c, a.calendar)?,
                    q,
                    dc,
                    c.get::<Shared<Settings<Date>>>(a.settings)?,
                    jumps,
                    dates,
                )?
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
                density: None,
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
                density: None,
            };
            output(out, c.insert(curve)?)
        })
    }
}
/// A zero settings handle selects the fixed reference date; a nonzero one
/// selects a reference date settlement_days business days on calendar past
/// the evaluation date of settings. settings must be the SAME instance the
/// helpers carry: a different instance moves the curve's reference date while
/// the helpers stay put.
#[repr(C)]
pub struct ItofinPiecewiseDefaultConfig {
    pub reference_date: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub day_counter: u64,
    pub settings: u64,
}
/// The itofin_piecewise_default_new twin taking a fixed or moving reference
/// date and multiplicative survival jumps. Empty dates generate year-end jump
/// dates from the reference year, which on a moving curve needs the
/// evaluation date already set: an unset one is an error, never a clock
/// fallback.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_default_with_jumps_new(
    ctx: *mut Context,
    config: *const ItofinPiecewiseDefaultConfig,
    helpers: *const u64,
    helper_count: usize,
    jumps: *const u64,
    jump_count: usize,
    dates: *const i32,
    date_count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            let helpers = input_slice(helpers, helper_count)?
                .iter()
                .map(|id| c.get::<Shared<dyn DefaultProbabilityHelper>>(*id))
                .collect::<BindingResult<Vec<_>>>()?;
            let jumps = input_slice(jumps, jump_count)?
                .iter()
                .map(|id| {
                    c.get::<Shared<SimpleQuote>>(*id)
                        .map(|q| Handle::new(q as Shared<dyn Quote>))
                })
                .collect::<BindingResult<Vec<_>>>()?;
            let dates = input_slice(dates, date_count)?
                .iter()
                .map(|d| date(*d))
                .collect::<BindingResult<Vec<_>>>()?;
            let dc = day_counter(c, a.day_counter)?;
            let concrete = if a.settings == 0 {
                Piecewise::with_jumps(
                    date(a.reference_date)?,
                    helpers,
                    dc,
                    BackwardFlat,
                    jumps,
                    dates,
                )?
            } else {
                Piecewise::moving_with_jumps(
                    a.settlement_days,
                    crate::time_api::calendar(c, a.calendar)?,
                    helpers,
                    dc,
                    BackwardFlat,
                    c.get::<Shared<Settings<Date>>>(a.settings)?,
                    jumps,
                    dates,
                )?
            };
            let curve = CreditCurve {
                handle: Handle::new(concrete.clone()),
                interpolated: None,
                piecewise: Some(concrete),
                density: None,
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
pub unsafe extern "C" fn itofin_interpolated_default_density_new(
    ctx: *mut Context,
    dates: *const i32,
    densities: *const f64,
    count: usize,
    dc: u64,
    calendar: u64,
    interpolation: i32,
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
            let densities = input_slice(densities, count)?.to_vec();
            let dc = day_counter(c, dc)?;
            let calendar = if calendar == 0 {
                None
            } else {
                Some(crate::time_api::calendar(c, calendar)?)
            };
            let (handle, density): (Handle<dyn DefaultProbabilityTermStructure>, _) =
                match interpolation {
                    0 => {
                        let p = shared(InterpolatedDefaultDensityCurve::with_calendar(
                            dates,
                            densities,
                            dc,
                            calendar,
                            BackwardFlat,
                        )?);
                        (Handle::new(p.clone()), DensityCurve::Backward(p))
                    }
                    1 => {
                        let p = shared(InterpolatedDefaultDensityCurve::with_calendar(
                            dates, densities, dc, calendar, Linear,
                        )?);
                        (Handle::new(p.clone()), DensityCurve::Linear(p))
                    }
                    _ => {
                        return Err(BindingError::invalid(
                            "unknown default density interpolation",
                        ));
                    }
                };
            output(
                out,
                c.insert(CreditCurve {
                    handle,
                    interpolated: None,
                    piecewise: None,
                    density: Some(density),
                })?,
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
pub unsafe extern "C" fn itofin_piecewise_default_density_new(
    ctx: *mut Context,
    reference: i32,
    helpers: *const u64,
    count: usize,
    dc: u64,
    interpolation: i32,
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
            let reference = date(reference)?;
            let dc = day_counter(c, dc)?;
            let (handle, density): (Handle<dyn DefaultProbabilityTermStructure>, _) =
                match interpolation {
                    0 => {
                        let p = PiecewiseDefaultCurve::<DefaultDensity, _>::new(
                            reference,
                            helpers,
                            dc,
                            BackwardFlat,
                        )?;
                        (Handle::new(p.clone()), DensityCurve::PiecewiseBackward(p))
                    }
                    1 => {
                        let p = PiecewiseDefaultCurve::<DefaultDensity, _>::new(
                            reference, helpers, dc, Linear,
                        )?;
                        (Handle::new(p.clone()), DensityCurve::PiecewiseLinear(p))
                    }
                    _ => {
                        return Err(BindingError::invalid(
                            "unknown default density interpolation",
                        ));
                    }
                };
            output(
                out,
                c.insert(CreditCurve {
                    handle,
                    interpolated: None,
                    piecewise: None,
                    density: Some(density),
                })?,
            )
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
            } else if let Some(p) = curve.density {
                p.nodes()?
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
            let curve = c.get::<CreditCurve>(id)?;
            if let Some(p) = curve.piecewise {
                p.calculate()?;
                Ok(())
            } else if let Some(p) = curve.density {
                p.calculate()
            } else {
                Err(BindingError::invalid("not a piecewise default curve"))
            }
        })
    }
}

/// Capacity zero queries the required length. Dates remain fixed when reference dates move.
/// A null `times` pointer requests dates only, without resolving the reference date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_default_curve_jumps(
    ctx: *mut Context,
    id: u64,
    dates: *mut i32,
    times: *mut f64,
    capacity: usize,
    count: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(count)?;
            let curve = c.get::<CreditCurve>(id)?.handle.current_link()?;
            let jump_dates = curve.jump_dates();
            output(count, jump_dates.len())?;
            if capacity == 0 {
                return Ok(());
            }
            check_ptr(dates)?;
            if capacity < jump_dates.len() {
                return Err(BindingError::invalid("jump buffer too small"));
            }
            if !times.is_null() {
                check_ptr(times)?;
                for (i, t) in curve.jump_times()?.into_iter().enumerate() {
                    output(times.add(i), t)?;
                }
            }
            for (i, d) in jump_dates.iter().enumerate() {
                output(dates.add(i), d.serial_number())?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar_api::NativeCalendar;
    use libitofin::interestrate::Compounding;
    use libitofin::termstructures::credit::defaultprobabilityhelpers::SpreadCdsHelper;
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    use libitofin::time::businessdayconvention::BusinessDayConvention;
    use libitofin::time::calendars::{NullCalendar, Target};
    use libitofin::time::date::Month;
    use libitofin::time::dategenerationrule::DateGeneration;
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use libitofin::time::frequency::Frequency;
    use libitofin::time::period::Period;
    use libitofin::time::timeunit::TimeUnit;

    /// The same-Settings contract reaches the unset-date errors only after a
    /// reset: helpers need the date at construction, and only the core can
    /// clear it.
    #[test]
    fn moving_piecewise_on_the_helpers_reset_settings_errs_until_the_date_returns() {
        let mut c = Context::new();
        let today = Date::new(15, Month::June, 2026);
        let dc = Actual365Fixed::new();
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        settings.set_include_todays_cash_flows(Some(true));
        let discount = Handle::new(shared(FlatForward::with_rate(
            today,
            0.03,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let helper = SpreadCdsHelper::new(
            Handle::new(shared(SimpleQuote::new(0.3))),
            Period::new(1, TimeUnit::Years),
            1,
            Target::new(),
            Frequency::Quarterly,
            BusinessDayConvention::Following,
            DateGeneration::TwentiethIMM,
            dc.clone(),
            0.4,
            discount,
            settings.clone(),
        )
        .unwrap() as Shared<dyn DefaultProbabilityHelper>;
        let helpers = [c.insert(helper).unwrap()];
        let jumps = [c.insert(shared(SimpleQuote::new(0.9))).unwrap()];
        let jump_date = (today + 100).serial_number();
        let config = ItofinPiecewiseDefaultConfig {
            reference_date: 0,
            settlement_days: 0,
            calendar: c
                .insert(NativeCalendar {
                    inner: NullCalendar::new(),
                    horizon: None,
                    first_year: 1901,
                })
                .unwrap(),
            day_counter: c.insert(dc).unwrap(),
            settings: c.insert(settings.clone()).unwrap(),
        };
        let null = std::ptr::null_mut();
        settings.reset_evaluation_date();
        let (mut jumped, mut plain, mut v) = (0, 0, 0.0);
        unsafe {
            let new = |c: &mut Context, jumps: &[u64], dates: &[i32], out: &mut u64| {
                itofin_piecewise_default_with_jumps_new(
                    c,
                    &config,
                    helpers.as_ptr(),
                    helpers.len(),
                    jumps.as_ptr(),
                    jumps.len(),
                    dates.as_ptr(),
                    dates.len(),
                    out,
                    null,
                )
            };
            assert_ne!(new(&mut c, &jumps, &[], &mut jumped), 0);
            assert_eq!(new(&mut c, &jumps, &[jump_date], &mut jumped), 0);
            assert_eq!(new(&mut c, &[], &[], &mut plain), 0);
            for id in [jumped, plain] {
                let read = itofin_default_curve_value(&mut c, id, 0, 1.0, 0, 0, 0, &mut v, null);
                assert_ne!(read, 0);
            }
            settings.set_evaluation_date(today);
            for id in [jumped, plain] {
                let read = itofin_default_curve_value(&mut c, id, 0, 1.0, 0, 0, 0, &mut v, null);
                assert_eq!(read, 0);
                assert!(v > 0.0 && v < 1.0);
            }
        }
    }

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
