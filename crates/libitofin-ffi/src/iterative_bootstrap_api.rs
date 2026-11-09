use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::math::interpolations::Interpolator;
use libitofin::math::interpolations::{
    convexmonotone::ConvexMonotone, cubic::Cubic, flat::BackwardFlat, linear::Linear,
    loglinear::LogLinear,
};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::Shared;
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::bootstraptraits::{
    Discount, ForwardRate, YieldBootstrapTraits, ZeroYield,
};
use libitofin::termstructures::iterativebootstrap::{
    Bootstrap, IterativeBootstrap, IterativeBootstrapOptions,
};
use libitofin::termstructures::yields::PiecewiseYieldCurve;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{calendar::Calendar, date::Date, daycounter::DayCounter};
use libitofin::types::Natural;

/// Initialize with itofin_iterative_bootstrap_options_default before overriding.
/// Presence and dont_throw flags accept only 0/1. Absent scalar values are ignored.
/// dont_throw opts into approximate curves; helper evaluation errors still fail.
#[repr(C)]
pub struct ItofinIterativeBootstrapOptions {
    pub accuracy: f64,
    pub min_value: f64,
    pub max_value: f64,
    pub has_accuracy: i32,
    pub has_min_value: i32,
    pub has_max_value: i32,
    pub max_attempts: usize,
    pub max_factor: f64,
    pub min_factor: f64,
    pub dont_throw: i32,
    pub dont_throw_steps: usize,
    pub max_evaluations: usize,
}
impl ItofinIterativeBootstrapOptions {
    pub(crate) fn parse(&self) -> BindingResult<IterativeBootstrapOptions> {
        let flag = |x| match x {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(BindingError::invalid("bootstrap flags must be 0 or 1")),
        };
        let value = IterativeBootstrapOptions {
            accuracy: flag(self.has_accuracy)?.then_some(self.accuracy),
            min_value: flag(self.has_min_value)?.then_some(self.min_value),
            max_value: flag(self.has_max_value)?.then_some(self.max_value),
            max_attempts: self.max_attempts,
            max_factor: self.max_factor,
            min_factor: self.min_factor,
            dont_throw: flag(self.dont_throw)?,
            dont_throw_steps: self.dont_throw_steps,
            max_evaluations: self.max_evaluations,
        };
        value.validate()?;
        Ok(value)
    }
}

/// Write the strict, trait-bounded bootstrap defaults without creating handles.
/// # Safety
/// Output and error pointers must follow the crate C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_iterative_bootstrap_options_default(
    out: *mut ItofinIterativeBootstrapOptions,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            let x = IterativeBootstrapOptions::default();
            output(
                out,
                ItofinIterativeBootstrapOptions {
                    accuracy: 0.0,
                    min_value: 0.0,
                    max_value: 0.0,
                    has_accuracy: 0,
                    has_min_value: 0,
                    has_max_value: 0,
                    max_attempts: x.max_attempts,
                    max_factor: x.max_factor,
                    min_factor: x.min_factor,
                    dont_throw: 0,
                    dont_throw_steps: x.dont_throw_steps,
                    max_evaluations: x.max_evaluations,
                },
            )
        })
    }
}

/// The reference date of a piecewise curve: fixed, or moving with the
/// evaluation date of `settings`, `settlement_days` business days on `calendar`.
pub(crate) enum CurveAnchor {
    Fixed(Date),
    Moving(Natural, Calendar, Shared<Settings<Date>>),
}

/// Multiplicative discount jump quotes and their dates; both empty is no jumps.
#[derive(Default)]
pub(crate) struct CurveJumps(pub(crate) Vec<Handle<dyn Quote>>, pub(crate) Vec<Date>);

/// Reads jump SimpleQuote handles and date serials from C arrays.
pub(crate) unsafe fn curve_jumps(
    c: &Context,
    jumps: *const u64,
    jump_count: usize,
    dates: *const i32,
    date_count: usize,
) -> BindingResult<CurveJumps> {
    unsafe {
        let quotes = input_slice(jumps, jump_count)?
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
        Ok(CurveJumps(quotes, dates))
    }
}

pub(crate) fn anchored<T, I, B>(
    anchor: CurveAnchor,
    helpers: Vec<Shared<dyn RateHelper>>,
    dc: DayCounter,
    interpolator: I,
    bootstrap: B,
    jumps: CurveJumps,
) -> BindingResult<Shared<dyn YieldTermStructure>>
where
    T: YieldBootstrapTraits + 'static,
    I: Interpolator + 'static,
    B: Bootstrap<PiecewiseYieldCurve<T, I, B>> + 'static,
{
    let CurveJumps(quotes, dates) = jumps;
    Ok(match anchor {
        CurveAnchor::Fixed(reference) => PiecewiseYieldCurve::<T, I, B>::with_bootstrap_and_jumps(
            reference,
            helpers,
            dc,
            interpolator,
            bootstrap,
            quotes,
            dates,
        )?,
        CurveAnchor::Moving(days, calendar, settings) => {
            PiecewiseYieldCurve::<T, I, B>::moving_with_bootstrap_and_jumps(
                days,
                calendar,
                helpers,
                dc,
                interpolator,
                settings,
                bootstrap,
                quotes,
                dates,
            )?
        }
    })
}

pub(crate) fn iterative_curve(
    anchor: CurveAnchor,
    helpers: Vec<Shared<dyn RateHelper>>,
    dc: DayCounter,
    kind: i32,
    options: IterativeBootstrapOptions,
    j: CurveJumps,
) -> BindingResult<Shared<dyn YieldTermStructure>> {
    let strategy = IterativeBootstrap::with_options(options)?;
    let (h, a) = (helpers, anchor);
    match kind {
        0 => anchored::<Discount, _, _>(a, h, dc, LogLinear, strategy, j),
        1 => anchored::<Discount, _, _>(a, h, dc, Linear, strategy, j),
        2 => anchored::<Discount, _, _>(a, h, dc, Cubic, strategy, j),
        3 => anchored::<ZeroYield, _, _>(a, h, dc, Linear, strategy, j),
        4 => anchored::<ZeroYield, _, _>(a, h, dc, Cubic, strategy, j),
        5 => anchored::<ForwardRate, _, _>(a, h, dc, Linear, strategy, j),
        6 => anchored::<ForwardRate, _, _>(a, h, dc, ConvexMonotone::default(), strategy, j),
        7 => anchored::<ForwardRate, _, _>(a, h, dc, BackwardFlat, strategy, j),
        _ => Err(BindingError::invalid("unsupported iterative curve kind")),
    }
}

/// Construct an iterative yield curve; kind uses itofin_piecewise_curve_new values.
/// Options are copied. Curves retain helpers and their dependencies.
/// # Safety
/// Pointers and arrays must follow the crate C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_piecewise_curve_new_with_options(
    ctx: *mut Context,
    reference: i32,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    options: *const ItofinIterativeBootstrapOptions,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let anchor = CurveAnchor::Fixed(date(reference)?);
            options_curve(c, anchor, helpers, len, dc, kind, options, out)
        })
    }
}

/// The itofin_piecewise_curve_new_with_options twin whose reference date moves
/// with the evaluation date of settings, settlement_days business days on
/// calendar; settings must be the instance the helpers' index carries.
/// # Safety
/// Pointers and arrays must follow the crate C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_piecewise_curve_new_with_options_moving(
    ctx: *mut Context,
    settlement_days: u32,
    calendar: u64,
    settings: u64,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    options: *const ItofinIterativeBootstrapOptions,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let anchor = moving_anchor(c, settlement_days, calendar, settings)?;
            options_curve(c, anchor, helpers, len, dc, kind, options, out)
        })
    }
}

pub(crate) fn moving_anchor(
    c: &Context,
    settlement_days: u32,
    calendar: u64,
    settings: u64,
) -> BindingResult<CurveAnchor> {
    Ok(CurveAnchor::Moving(
        settlement_days,
        crate::time_api::calendar(c, calendar)?,
        c.get::<Shared<Settings<Date>>>(settings)?,
    ))
}

#[allow(clippy::too_many_arguments)]
unsafe fn options_curve(
    c: &mut Context,
    anchor: CurveAnchor,
    helpers: *const u64,
    len: usize,
    dc: u64,
    kind: i32,
    options: *const ItofinIterativeBootstrapOptions,
    out: *mut u64,
) -> BindingResult<()> {
    unsafe {
        check_ptr(out)?;
        check_ptr(options)?;
        let options = (*options).parse()?;
        let dc = day_counter(c, dc)?;
        let helpers = input_slice(helpers, len)?
            .iter()
            .map(|id| crate::helpers_api::helper(c, *id))
            .collect::<BindingResult<Vec<_>>>()?;
        let curve = iterative_curve(anchor, helpers, dc, kind, options, CurveJumps::default())?;
        output(out, c.insert(Handle::new(curve))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::indexes::Euribor;
    use libitofin::settings::Settings;
    use libitofin::shared::shared;
    use libitofin::termstructures::yields::DepositRateHelper;
    use libitofin::time::{date::Month, daycounters::actual360::Actual360};
    use std::mem::MaybeUninit;
    use std::ptr::{null, null_mut};

    #[test]
    fn iterative_options_flags_and_null_pointers_do_not_poison_context() {
        let mut context = Context::new();
        let reference = Date::new(15, Month::June, 2026);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference);
        let index = Euribor::three_months(Handle::empty(), settings);
        let helper = context
            .insert(DepositRateHelper::from_rate(0.05, &index) as Shared<dyn RateHelper>)
            .unwrap();
        let dc = context.insert(Actual360::new()).unwrap();
        let mut value = MaybeUninit::uninit();
        unsafe {
            assert_eq!(
                itofin_iterative_bootstrap_options_default(null_mut(), null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_iterative_bootstrap_options_default(value.as_mut_ptr(), null_mut()),
                0
            );
            let mut value = value.assume_init();
            let parsed = value.parse().unwrap();
            assert_eq!(parsed.max_attempts, 1);
            assert_eq!(parsed.dont_throw_steps, 10);
            assert_eq!(parsed.max_evaluations, 100);
            assert_eq!((parsed.min_factor, parsed.max_factor), (2.0, 2.0));
            assert!(
                parsed.accuracy.is_none()
                    && parsed.min_value.is_none()
                    && parsed.max_value.is_none()
            );
            assert!(!parsed.dont_throw);
            let mut out = 123;
            assert_eq!(
                itofin_piecewise_curve_new_with_options(
                    &mut context,
                    0,
                    null(),
                    0,
                    0,
                    0,
                    &value,
                    null_mut(),
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_piecewise_curve_new_with_options(
                    &mut context,
                    0,
                    null(),
                    0,
                    0,
                    0,
                    null(),
                    &mut out,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            for selector in 0..4 {
                value.accuracy = 1e-12;
                value.min_value = 0.5;
                value.max_value = 1.5;
                let flag = match selector {
                    0 => &mut value.has_accuracy,
                    1 => &mut value.has_min_value,
                    2 => &mut value.has_max_value,
                    _ => &mut value.dont_throw,
                };
                *flag = 2;
                assert_eq!(
                    itofin_piecewise_curve_new_with_options(
                        &mut context,
                        reference.serial_number(),
                        &helper,
                        1,
                        dc,
                        0,
                        &value,
                        &mut out,
                        null_mut()
                    ),
                    INVALID_ARGUMENT
                );
                assert_eq!(out, 123);
                assert_eq!(
                    itofin_iterative_bootstrap_options_default(&mut value, null_mut()),
                    0
                );
            }
            assert_eq!(
                itofin_piecewise_curve_new_with_options(
                    &mut context,
                    reference.serial_number(),
                    &helper,
                    1,
                    dc,
                    0,
                    &value,
                    &mut out,
                    null_mut()
                ),
                0
            );
            let curve = context.get::<Handle<dyn YieldTermStructure>>(out).unwrap();
            let discount = curve.current_link().unwrap().discount(0.1, false).unwrap();
            assert!(discount.is_finite() && discount > 0.0 && discount < 1.0);
        }
    }
}
