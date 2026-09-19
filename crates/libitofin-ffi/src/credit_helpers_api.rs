//! Explicit model and contract terms for CDS bootstrap helpers.
use crate::boundary::*;
use crate::credit_instruments_api::ItofinSpreadCdsConfig;
use crate::time_api::{calendar, convention, date, day_counter, frequency, generation, time_unit};
use libitofin::handle::Handle;
use libitofin::instruments::PricingModel;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::Shared;
use libitofin::termstructures::credit::defaultprobabilityhelpers::{
    CdsHelperTerms, DefaultProbabilityHelper, SpreadCdsHelper, UpfrontCdsHelper,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Date, period::Period};

#[repr(C)]
pub struct ItofinCdsHelperTerms {
    pub model: i32,
    pub settles_accrual: i32,
    pub pays_at_default_time: i32,
    pub start_date: i32,
    pub last_period_day_counter: u64,
    pub rebates_accrual: i32,
}

fn boolean(value: i32) -> BindingResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("boolean must be 0 or 1")),
    }
}

fn terms(c: &Context, a: &ItofinCdsHelperTerms) -> BindingResult<CdsHelperTerms> {
    Ok(CdsHelperTerms {
        model: match a.model {
            0 => PricingModel::Midpoint,
            1 => PricingModel::Isda,
            _ => return Err(BindingError::invalid("unknown CDS pricing model")),
        },
        settles_accrual: boolean(a.settles_accrual)?,
        pays_at_default_time: boolean(a.pays_at_default_time)?,
        start_date: if a.start_date == 0 {
            None
        } else {
            Some(date(a.start_date)?)
        },
        last_period_day_counter: if a.last_period_day_counter == 0 {
            None
        } else {
            Some(day_counter(c, a.last_period_day_counter)?)
        },
        rebates_accrual: boolean(a.rebates_accrual)?,
    })
}

#[unsafe(no_mangle)]
/// Construct a spread (kind 0) or upfront (kind 1) helper with explicit terms.
/// The config quote is the running spread for kind 0 and upfront for kind 1.
///
/// # Safety
/// Pointers must be aligned, live and valid. Output must not overlap inputs.
/// The context and its handles must belong to the calling thread.
pub unsafe extern "C" fn itofin_cds_helper_with_terms(
    ctx: *mut Context,
    config: *const ItofinSpreadCdsConfig,
    contract_terms: *const ItofinCdsHelperTerms,
    kind: i32,
    running_spread: f64,
    upfront_settlement_days: u32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(contract_terms)?;
            check_ptr(out)?;
            let a = &*config;
            if !a.recovery.is_finite() || !(0.0..=1.0).contains(&a.recovery) {
                return Err(BindingError::invalid(
                    "recovery must be finite and in [0, 1]",
                ));
            }
            let contract_terms = terms(c, &*contract_terms)?;
            let quote: Handle<dyn Quote> = Handle::new(c.get::<Shared<SimpleQuote>>(a.quote)?);
            let tenor = Period::new(a.tenor_length, time_unit(a.tenor_unit)?);
            let calendar = calendar(c, a.calendar)?;
            let frequency = frequency(a.frequency)?;
            let convention = convention(a.convention)?;
            let rule = generation(a.rule)?;
            let day_counter = day_counter(c, a.day_counter)?;
            let discount = c.get::<Handle<dyn YieldTermStructure>>(a.discount)?;
            let settings = c.get::<Shared<Settings<Date>>>(a.settings)?;
            let helper: Shared<dyn DefaultProbabilityHelper> = match kind {
                0 => SpreadCdsHelper::with_terms(
                    quote,
                    tenor,
                    a.settlement_days,
                    calendar,
                    frequency,
                    convention,
                    rule,
                    day_counter,
                    a.recovery,
                    discount,
                    contract_terms,
                    settings,
                )?,
                1 => {
                    if !running_spread.is_finite() {
                        return Err(BindingError::invalid("running spread must be finite"));
                    }
                    UpfrontCdsHelper::with_terms(
                        quote,
                        running_spread,
                        tenor,
                        a.settlement_days,
                        calendar,
                        frequency,
                        convention,
                        rule,
                        day_counter,
                        a.recovery,
                        discount,
                        upfront_settlement_days,
                        contract_terms,
                        settings,
                    )?
                }
                _ => return Err(BindingError::invalid("unknown CDS helper kind")),
            };
            output(out, c.insert(helper)?)
        })
    }
}

#[unsafe(no_mangle)]
/// Return a helper's implied spread or upfront after recalculation.
///
/// # Safety
/// Pointers must be aligned, live and valid. Output must not overlap inputs.
/// The context and its handles must belong to the calling thread.
pub unsafe extern "C" fn itofin_cds_helper_implied_quote(
    ctx: *mut Context,
    id: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let helper = c.get::<Shared<dyn DefaultProbabilityHelper>>(id)?;
            output(out, helper.implied_quote()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_terms_validate_models_flags_dates_and_handles() {
        let ctx = Context::new();
        let mut config = ItofinCdsHelperTerms {
            model: 1,
            settles_accrual: 1,
            pays_at_default_time: 1,
            start_date: 0,
            last_period_day_counter: 0,
            rebates_accrual: 1,
        };
        assert!(terms(&ctx, &config).is_ok());
        config.model = 2;
        assert!(terms(&ctx, &config).is_err());
        config.model = 1;
        config.settles_accrual = 2;
        assert!(terms(&ctx, &config).is_err());
        config.settles_accrual = 1;
        config.start_date = -1;
        assert!(terms(&ctx, &config).is_err());
        config.start_date = 0;
        config.last_period_day_counter = u64::MAX;
        assert!(terms(&ctx, &config).is_err());
    }
}
