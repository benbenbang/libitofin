//! Vanilla option adapters. All valuation and lazy invalidation stay in the core.
use crate::boundary::*;
use crate::time_api::date;
use libitofin::exercise::{AmericanExercise, EuropeanExercise, Exercise, ExerciseType};
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    AssetOrNothingPayoff, CashOrNothingPayoff, GapPayoff, PercentageStrikePayoff,
    PlainVanillaPayoff, StrikedTypePayoff, VanillaOption,
};
use libitofin::models::HestonModel;
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::AnalyticEuropeanEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::time::date::Date;

pub(crate) fn option_type(value: i32) -> BindingResult<OptionType> {
    match value {
        0 => Ok(OptionType::Call),
        1 => Ok(OptionType::Put),
        _ => Err(BindingError::invalid("unknown option type")),
    }
}

/// `american`: 0 European, 1 American; earliest ignored for European exercise.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_option_new(
    ctx: *mut Context,
    kind: i32,
    strike: f64,
    earliest: i32,
    expiry: i32,
    american: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let payoff = shared(PlainVanillaPayoff::new(option_type(kind)?, strike))
                as Shared<dyn StrikedTypePayoff>;
            let exercise: Shared<dyn Exercise> = match american {
                0 => shared(EuropeanExercise::new(date(expiry)?)),
                1 => shared(AmericanExercise::over(date(earliest)?, date(expiry)?)?),
                _ => return Err(BindingError::invalid("american must be 0 or 1")),
            };
            let option =
                VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
            output(out, c.insert(shared_mut(option))?)
        })
    }
}

/// Build a European cash-or-nothing option retaining the supplied settings.
/// Kind is 0 call or 1 put. Strike must be finite and nonnegative; cash must
/// be finite, with signed and zero amounts accepted. Attach the existing analytic
/// European engine. Other engine compatibility is not established by this factory.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_cash_or_nothing_new(
    ctx: *mut Context,
    kind: i32,
    strike: f64,
    cash_payoff: f64,
    expiry: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !strike.is_finite() || strike < 0.0 || !cash_payoff.is_finite() {
                return Err(BindingError::invalid(
                    "strike must be finite and nonnegative; cash payoff must be finite",
                ));
            }
            let payoff = shared(CashOrNothingPayoff::new(
                option_type(kind)?,
                strike,
                cash_payoff,
            )) as Shared<dyn StrikedTypePayoff>;
            let exercise = shared(EuropeanExercise::new(date(expiry)?)) as Shared<dyn Exercise>;
            let option =
                VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
            output(out, c.insert(shared_mut(option))?)
        })
    }
}

/// Build a European asset-or-nothing option retaining the supplied settings.
/// Kind is 0 call or 1 put; strike must be finite and nonnegative. The payoff
/// pays the terminal asset price strictly beyond strike, and zero at equality.
/// Attach the existing FD Black-Scholes engine; the analytic engine rejects it.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_asset_or_nothing_new(
    ctx: *mut Context,
    kind: i32,
    strike: f64,
    expiry: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !strike.is_finite() || strike < 0.0 {
                return Err(BindingError::invalid(
                    "strike must be finite and nonnegative",
                ));
            }
            let payoff = shared(AssetOrNothingPayoff::new(option_type(kind)?, strike))
                as Shared<dyn StrikedTypePayoff>;
            let exercise = shared(EuropeanExercise::new(date(expiry)?)) as Shared<dyn Exercise>;
            let option =
                VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
            output(out, c.insert(shared_mut(option))?)
        })
    }
}

/// Build a European gap option with a separate trigger and payment strike.
/// Kind is 0 call or 1 put. The trigger strike must be finite and nonnegative;
/// the payment strike must be finite and may be signed. Trigger equality is
/// included and negative payments are not floored. Use the existing FD engine;
/// the analytic European engine rejects this payoff.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_gap_new(
    ctx: *mut Context,
    kind: i32,
    strike: f64,
    second_strike: f64,
    expiry: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !strike.is_finite() || strike < 0.0 || !second_strike.is_finite() {
                return Err(BindingError::invalid(
                    "trigger strike must be finite and nonnegative; payment strike must be finite",
                ));
            }
            let payoff = shared(GapPayoff::new(option_type(kind)?, strike, second_strike))
                as Shared<dyn StrikedTypePayoff>;
            let exercise = shared(EuropeanExercise::new(date(expiry)?)) as Shared<dyn Exercise>;
            let option =
                VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
            output(out, c.insert(shared_mut(option))?)
        })
    }
}

/// Build a European percentage-strike option for the existing FD engine.
/// Moneyness is a finite dimensionless ratio, not an absolute strike or percent
/// scaled by 100. Signed ratios are accepted. Call pays S*max(1-m,0), put pays
/// S*max(m-1,0). FD volatility lookup uses m unchanged; certify constant-vol use.
/// The analytic Black engine rejects this payoff.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_percentage_strike_new(
    ctx: *mut Context,
    kind: i32,
    moneyness: f64,
    expiry: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !moneyness.is_finite() {
                return Err(BindingError::invalid("moneyness must be finite"));
            }
            let payoff = shared(PercentageStrikePayoff::new(option_type(kind)?, moneyness))
                as Shared<dyn StrikedTypePayoff>;
            let exercise = shared(EuropeanExercise::new(date(expiry)?)) as Shared<dyn Exercise>;
            let option =
                VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
            output(out, c.insert(shared_mut(option))?)
        })
    }
}

/// Build an American option whose exercise window starts at Date::min_date().
/// # Safety
/// Outputs must be aligned, live and non-overlapping. Context and handles must
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_american_until_new(
    ctx: *mut Context,
    kind: i32,
    strike: f64,
    expiry: i32,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let payoff = shared(PlainVanillaPayoff::new(option_type(kind)?, strike))
                as Shared<dyn StrikedTypePayoff>;
            let exercise =
                shared(AmericanExercise::until(date(expiry)?, false)?) as Shared<dyn Exercise>;
            let option =
                VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
            output(out, c.insert(shared_mut(option))?)
        })
    }
}

/// Build a vanilla option retaining a Bermudan exercise handle.
/// # Safety
/// Outputs must be aligned, live and non-overlapping. Context and handles must
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_bermudan_new(
    ctx: *mut Context,
    kind: i32,
    strike: f64,
    exercise: u64,
    settings: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let payoff = shared(PlainVanillaPayoff::new(option_type(kind)?, strike))
                as Shared<dyn StrikedTypePayoff>;
            let exercise = c.get::<Shared<dyn Exercise>>(exercise)?;
            if exercise.exercise_type() != ExerciseType::Bermudan {
                return Err(BindingError::invalid("Bermudan exercise required"));
            }
            let option =
                VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
            output(out, c.insert(shared_mut(option))?)
        })
    }
}

/// Engine kind: 0 analytic European (BSM process), 1 analytic Heston (model),
/// 2 preconstructed pricing engine (including MC and FD).
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_option_set_engine(
    ctx: *mut Context,
    option: u64,
    source: u64,
    kind: i32,
    integration_order: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let option = c.get::<SharedMut<VanillaOption>>(option)?;
            let engine: SharedMut<dyn PricingEngine> =
                match kind {
                    0 => shared_mut(AnalyticEuropeanEngine::new(c.get::<Shared<
                        GeneralizedBlackScholesProcess,
                    >>(
                        source
                    )?)),
                    1 => shared_mut(AnalyticHestonEngine::new(
                        c.get::<SharedMut<HestonModel>>(source)?,
                        integration_order,
                    )?),
                    2 => c.get::<SharedMut<dyn PricingEngine>>(source).or_else(|_| {
                        c.get::<SharedMut<
                            libitofin::pricingengines::vanilla::coshestonengine::CosHestonEngine,
                        >>(source)
                            .map(|engine| engine as SharedMut<dyn PricingEngine>)
                    })?,
                    _ => return Err(BindingError::invalid("unknown option engine kind")),
                };
            option.borrow_mut().base_mut().set_pricing_engine(engine);
            Ok(())
        })
    }
}

/// Field: 0 NPV, 1 delta, 2 gamma, 3 theta, 4 vega, 5 rho, 6 dividend rho,
/// 7 error estimate, 8 exercise probability. Missing fields return core errors.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_option_value(
    ctx: *mut Context,
    option: u64,
    field: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let option = c.get::<SharedMut<VanillaOption>>(option)?;
            let mut option = option.borrow_mut();
            let value = match field {
                0 => option.npv()?,
                1 => option.delta()?,
                2 => option.gamma()?,
                3 => option.theta()?,
                4 => option.vega()?,
                5 => option.rho()?,
                6 => option.dividend_rho()?,
                7 => option.error_estimate()?,
                8 => option.result::<f64>("exerciseProbability")?,
                _ => return Err(BindingError::invalid("unknown option result field")),
            };
            output(out, value)
        })
    }
}

/// Calculate if requested, then report cache validity.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_option_calculate(
    ctx: *mut Context,
    option: u64,
    calculate: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let option = c.get::<SharedMut<VanillaOption>>(option)?;
            let mut option = option.borrow_mut();
            match calculate {
                0 => (),
                1 => option.calculate()?,
                _ => return Err(BindingError::invalid("calculate must be 0 or 1")),
            }
            output(out, i32::from(option.base().is_calculated()))
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_option_results(
    ctx: *mut Context,
    option: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let option = c.get::<SharedMut<VanillaOption>>(option)?;
            let mut option = option.borrow_mut();
            option.calculate()?;
            output(out, c.insert(crate::results_api::snapshot(option.base()))?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::time::date::Month;
    use std::ptr::null_mut;

    #[test]
    fn invalid_inputs_leave_output_untouched_and_context_usable() {
        let mut context = Context::new();
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(15, Month::June, 2026);
        settings.set_evaluation_date(today);
        let settings = context.insert(settings).unwrap();
        let mut id = 99;
        unsafe {
            assert_eq!(
                itofin_option_new(
                    &mut context,
                    0,
                    100.,
                    0,
                    today.serial_number(),
                    0,
                    settings,
                    null_mut(),
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_option_new(
                    &mut context,
                    9,
                    100.,
                    0,
                    today.serial_number(),
                    0,
                    settings,
                    &mut id,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(id, 99);
            assert_eq!(
                itofin_option_new(
                    &mut context,
                    0,
                    100.,
                    0,
                    -1,
                    0,
                    settings,
                    &mut id,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(id, 99);
            assert_eq!(
                itofin_option_new(
                    &mut context,
                    0,
                    100.,
                    0,
                    today.serial_number() + 360,
                    0,
                    settings,
                    &mut id,
                    null_mut()
                ),
                0
            );
            let mut foreign = Context::new();
            let mut value = 123.;
            assert_eq!(
                itofin_option_value(&mut foreign, id, 0, &mut value, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(value, 123.);
            assert_eq!(
                itofin_option_value(&mut context, id, 99, &mut value, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(value, 123.);
            assert_eq!(
                itofin_option_value(&mut context, id, 0, &mut value, null_mut()),
                CORE_ERROR
            );
            assert_eq!(value, 123.);
            assert_eq!(itofin_handle_release(&mut context, id, null_mut()), 0);
            assert_eq!(
                itofin_option_value(&mut context, id, 0, &mut value, null_mut()),
                INVALID_HANDLE
            );
        }
    }
}

#[cfg(test)]
#[path = "asset_digital_tests.rs"]
mod asset_digital_tests;

#[cfg(test)]
#[path = "gap_option_tests.rs"]
mod gap_option_tests;

#[cfg(test)]
#[path = "percentage_strike_tests.rs"]
mod percentage_strike_tests;
