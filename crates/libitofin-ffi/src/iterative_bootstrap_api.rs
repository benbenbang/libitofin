use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::math::interpolations::{
    convexmonotone::ConvexMonotone, cubic::Cubic, flat::BackwardFlat, linear::Linear,
    loglinear::LogLinear,
};
use libitofin::shared::Shared;
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::bootstraptraits::{Discount, ForwardRate, ZeroYield};
use libitofin::termstructures::iterativebootstrap::{
    IterativeBootstrap, IterativeBootstrapOptions,
};
use libitofin::termstructures::yields::PiecewiseYieldCurve;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Date, daycounter::DayCounter};

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
    fn parse(&self) -> BindingResult<IterativeBootstrapOptions> {
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

pub(crate) fn iterative_curve(
    reference: Date,
    helpers: Vec<Shared<dyn RateHelper>>,
    dc: DayCounter,
    kind: i32,
    options: IterativeBootstrapOptions,
) -> BindingResult<Shared<dyn YieldTermStructure>> {
    let strategy = IterativeBootstrap::with_options(options)?;
    let curve: Shared<dyn YieldTermStructure> = match kind {
        0 => PiecewiseYieldCurve::<Discount, LogLinear>::with_bootstrap(
            reference, helpers, dc, LogLinear, strategy,
        )?,
        1 => PiecewiseYieldCurve::<Discount, Linear>::with_bootstrap(
            reference, helpers, dc, Linear, strategy,
        )?,
        2 => PiecewiseYieldCurve::<Discount, Cubic>::with_bootstrap(
            reference, helpers, dc, Cubic, strategy,
        )?,
        3 => PiecewiseYieldCurve::<ZeroYield, Linear>::with_bootstrap(
            reference, helpers, dc, Linear, strategy,
        )?,
        4 => PiecewiseYieldCurve::<ZeroYield, Cubic>::with_bootstrap(
            reference, helpers, dc, Cubic, strategy,
        )?,
        5 => PiecewiseYieldCurve::<ForwardRate, Linear>::with_bootstrap(
            reference, helpers, dc, Linear, strategy,
        )?,
        6 => PiecewiseYieldCurve::<ForwardRate, ConvexMonotone>::with_bootstrap(
            reference,
            helpers,
            dc,
            ConvexMonotone::default(),
            strategy,
        )?,
        7 => PiecewiseYieldCurve::<ForwardRate, BackwardFlat>::with_bootstrap(
            reference,
            helpers,
            dc,
            BackwardFlat,
            strategy,
        )?,
        _ => return Err(BindingError::invalid("unsupported iterative curve kind")),
    };
    Ok(curve)
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
            check_ptr(out)?;
            check_ptr(options)?;
            let options = (*options).parse()?;
            let reference = date(reference)?;
            let dc = day_counter(c, dc)?;
            let helpers = input_slice(helpers, len)?
                .iter()
                .map(|id| crate::helpers_api::helper(c, *id))
                .collect::<BindingResult<Vec<_>>>()?;
            let curve = iterative_curve(reference, helpers, dc, kind, options)?;
            output(out, c.insert(Handle::new(curve))?)
        })
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
