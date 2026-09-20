//! Basis helpers and the bounded two-curve global assembly.
use crate::boundary::*;
use crate::curves_api::curve;
use crate::helpers_api::helper;
use crate::indexes_api::{ibor_index, period};
use crate::market_api::quote;
use crate::time_api::{calendar, convention, date, day_counter};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::yields::{BasisSwapHelperConfig, JointYieldCurves};

#[derive(Clone)]
pub(crate) struct BasisHelper {
    pub config: BasisSwapHelperConfig,
    pub helper: Shared<dyn RateHelper>,
}

/// Inputs for a standalone basis helper and reusable joint template.
#[repr(C)]
pub struct ItofinBasisHelperConfig {
    pub quote: u64,
    pub tenor_length: i32,
    pub tenor_unit: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub convention: i32,
    pub end_of_month: i32,
    pub base_index: u64,
    pub other_index: u64,
    pub discount_curve: u64,
    pub bootstrap_base_curve: i32,
}

fn flag(value: i32) -> BindingResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("basis flags must be zero or one")),
    }
}

/// Construct a basis helper retaining its quotes, indices and discount curve.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_basis_helper_new(
    ctx: *mut Context,
    cfg: *const ItofinBasisHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let cfg = &*cfg;
            let config = BasisSwapHelperConfig {
                quote: quote(c, cfg.quote)?,
                tenor: period(cfg.tenor_length, cfg.tenor_unit)?,
                settlement_days: cfg.settlement_days,
                calendar: calendar(c, cfg.calendar)?,
                convention: convention(cfg.convention)?,
                end_of_month: flag(cfg.end_of_month)?,
                base_index: ibor_index(c, cfg.base_index)?,
                other_index: ibor_index(c, cfg.other_index)?,
                discount: curve(c, cfg.discount_curve)?,
                bootstrap_base_curve: flag(cfg.bootstrap_base_curve)?,
            };
            let helper = config.build()? as Shared<dyn RateHelper>;
            output(out, c.insert(BasisHelper { config, helper })?)
        })
    }
}

/// Assemble two global discount/log-linear curves from plain strips and basis templates.
/// # Safety
/// Follow the crate-level pointer and thread contract. Each input slice contains
/// its stated number of live handles belonging to the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_joint_curves_new(
    ctx: *mut Context,
    reference: i32,
    first: *const u64,
    first_len: usize,
    second: *const u64,
    second_len: usize,
    basis: *const u64,
    basis_len: usize,
    dc: u64,
    accuracy: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let first = input_slice(first, first_len)?
                .iter()
                .map(|id| helper(c, *id))
                .collect::<BindingResult<Vec<_>>>()?;
            let second = input_slice(second, second_len)?
                .iter()
                .map(|id| helper(c, *id))
                .collect::<BindingResult<Vec<_>>>()?;
            let basis = input_slice(basis, basis_len)?
                .iter()
                .map(|id| c.get::<BasisHelper>(*id).map(|value| value.config))
                .collect::<BindingResult<Vec<_>>>()?;
            let joint = JointYieldCurves::new(
                date(reference)?,
                [first, second],
                &basis,
                day_counter(c, dc)?,
                accuracy,
            )?;
            output(out, c.insert(shared(joint))?)
        })
    }
}

/// Return member zero or one with retained ownership of both contributors.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_joint_curve(
    ctx: *mut Context,
    joint: u64,
    member: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let member = usize::try_from(member)
                .map_err(|_| BindingError::invalid("joint curve member must be 0 or 1"))?;
            let curve = c.get::<Shared<JointYieldCurves>>(joint)?.curve(member)?;
            output(out, c.insert(curve)?)
        })
    }
}

/// Construct a swap helper with an exogenous discount curve.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_swap_helper_with_discount(
    ctx: *mut Context,
    cfg: *const crate::helpers_api::ItofinSwapHelperConfig,
    discount: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let index = ibor_index(c, a.index)?;
            let tenor = period(a.tenor_length, a.tenor_unit)?;
            if tenor.length() <= 0 {
                return Err(BindingError::invalid("swap tenor must be positive"));
            }
            let quote = quote(c, a.quote)?;
            let calendar = calendar(c, a.calendar)?;
            let frequency = crate::time_api::frequency(a.frequency)?;
            let convention = convention(a.convention)?;
            let day_counter = day_counter(c, a.day_counter)?;
            let discount = curve(c, discount)?;
            let helper = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                libitofin::termstructures::yields::SwapRateHelper::with_details(
                    quote,
                    tenor,
                    calendar,
                    frequency,
                    convention,
                    day_counter,
                    &index,
                    libitofin::handle::Handle::empty(),
                    libitofin::time::period::Period::new(
                        0,
                        libitofin::time::timeunit::TimeUnit::Days,
                    ),
                    Some(discount),
                    libitofin::termstructures::yields::Pillar::LastRelevantDate,
                )
            }))
            .map_err(|_| {
                BindingError::invalid("swap schedule inputs exceed the supported date range")
            })?;
            output(out, c.insert(helper as Shared<dyn RateHelper>)?)
        })
    }
}

/// Add a fixing, rejecting conflicting existing values.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_ibor_add_fixing(
    ctx: *mut Context,
    index: u64,
    fixing_date: i32,
    value: f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            use libitofin::indexes::index::Index;
            if !value.is_finite() {
                return Err(BindingError::invalid("fixing must be finite"));
            }
            ibor_index(c, index)?.add_fixing(date(fixing_date)?, value)?;
            Ok(())
        })
    }
}

/// Construct a flat continuous discount curve retaining a live quote.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_flat_forward_from_quote(
    ctx: *mut Context,
    reference: i32,
    quote_id: u64,
    dc: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let curve = shared(libitofin::termstructures::yields::FlatForward::new(
                date(reference)?,
                quote(c, quote_id)?,
                day_counter(c, dc)?,
                libitofin::interestrate::Compounding::Continuous,
                libitofin::time::frequency::Frequency::Annual,
            ))
                as Shared<dyn libitofin::termstructures::yieldtermstructure::YieldTermStructure>;
            output(out, c.insert(libitofin::handle::Handle::new(curve))?)
        })
    }
}

#[cfg(test)]
#[path = "joint_curves_tests.rs"]
mod tests;
