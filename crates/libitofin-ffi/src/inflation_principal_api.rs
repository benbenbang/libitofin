//! Standalone explicit-base and live-base inflation principal cash flows.
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::cpi_cashflows_api::CpiLegSettings;
use crate::inflation_api::{bool_value, cpi_interpolation, zero_index};
use crate::rates_api::{finite, period};
use crate::time_api::date;
use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::{CpiCashFlow, ZeroInflationCashFlow};
use libitofin::indexes::inflationindex::InflationIndex;
#[cfg(test)]
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::businessdayconvention::BusinessDayConvention;
#[cfg(test)]
use libitofin::time::date::Date;

#[path = "inflation_principal_flow.rs"]
mod principal_flow;
use principal_flow::{FiniteCpiFlow, FiniteZeroFlow};

#[derive(Clone)]
enum PrincipalFlow {
    Explicit(Shared<FiniteCpiFlow>),
    Live(Shared<FiniteZeroFlow>),
}
impl PrincipalFlow {
    fn erased(&self) -> Shared<dyn CashFlow> {
        match self {
            Self::Explicit(flow) => flow.clone(),
            Self::Live(flow) => flow.clone(),
        }
    }
    fn value(&self, field: i32) -> BindingResult<f64> {
        finite(match (self, field) {
            (_, 0) => self.erased().amount()?,
            (Self::Explicit(flow), 1) => flow.base_fixing(),
            (Self::Live(flow), 1) => flow.base_fixing()?,
            (Self::Explicit(flow), 2) => flow.index_fixing()?,
            (Self::Live(flow), 2) => flow.index_fixing()?,
            _ => return Err(BindingError::invalid("unknown inflation principal query")),
        })
    }
}
#[derive(Clone)]
struct NativeInflationPrincipal {
    flow: PrincipalFlow,
    settings: CpiLegSettings,
}

/// Explicit base CPI is fixed; the observation remains live.
#[repr(C)]
pub struct ItofinCpiCashFlowConfig {
    pub index: u64,
    pub nominal: f64,
    pub base_cpi: f64,
    pub observation_date: i32,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub payment_date: i32,
    pub growth_only: i32,
}
/// Both lagged start and end CPI observations remain live.
#[repr(C)]
pub struct ItofinZeroInflationCashFlowConfig {
    pub index: u64,
    pub nominal: f64,
    pub start_date: i32,
    pub end_date: i32,
    pub lag_length: i32,
    pub lag_unit: i32,
    pub interpolation: i32,
    pub payment_date: i32,
    pub growth_only: i32,
}

#[unsafe(no_mangle)]
/// Construct a standalone explicit-base CPI principal without reading fixings.
/// # Safety
/// Pointers must be aligned and valid, and outputs must not overlap. Context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_cpi_cashflow_new(
    ctx: *mut Context,
    a: ItofinCpiCashFlowConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let index = zero_index(c, a.index)?;
            let settings = CpiLegSettings(index.inflation_base().settings().clone());
            let flow = CpiCashFlow::new(
                finite(a.nominal)?,
                index,
                finite(a.base_cpi)?,
                date(a.observation_date)?,
                period(a.lag_length, a.lag_unit)?,
                cpi_interpolation(a.interpolation)?,
                date(a.payment_date)?,
                bool_value(a.growth_only)?,
            )?;
            output(
                out,
                c.insert(NativeInflationPrincipal {
                    flow: PrincipalFlow::Explicit(shared(FiniteCpiFlow(flow))),
                    settings,
                })?,
            )
        })
    }
}

#[unsafe(no_mangle)]
/// Construct a standalone live-base inflation principal without reading fixings.
/// # Safety
/// Pointers must be aligned and valid, and outputs must not overlap. Context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_zero_inflation_cashflow_new(
    ctx: *mut Context,
    a: ItofinZeroInflationCashFlowConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if a.lag_length < 0 {
                return Err(BindingError::invalid("negative inflation observation lag"));
            }
            let lag = period(a.lag_length, a.lag_unit)?;
            let start = date(a.start_date)?;
            let end = date(a.end_date)?;
            let arithmetic = NativeCalendar {
                inner: libitofin::time::calendars::NullCalendar::new(),
                horizon: None,
                first_year: 1901,
            };
            for observation in [start, end] {
                crate::floating_bond_api::advance(
                    &arithmetic,
                    observation,
                    -a.lag_length,
                    lag.units(),
                    BusinessDayConvention::Unadjusted,
                    false,
                )?;
            }
            let index = zero_index(c, a.index)?;
            let settings = CpiLegSettings(index.inflation_base().settings().clone());
            let flow = ZeroInflationCashFlow::new(
                finite(a.nominal)?,
                index,
                cpi_interpolation(a.interpolation)?,
                start,
                end,
                lag,
                date(a.payment_date)?,
                bool_value(a.growth_only)?,
            );
            output(
                out,
                c.insert(NativeInflationPrincipal {
                    flow: PrincipalFlow::Live(shared(FiniteZeroFlow(flow))),
                    settings,
                })?,
            )
        })
    }
}

#[unsafe(no_mangle)]
/// Query 0 amount, 1 base fixing or 2 index fixing; fixings are read lazily.
/// # Safety
/// Pointers must be aligned and valid, and outputs must not overlap. Context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_inflation_cashflow_value(
    ctx: *mut Context,
    id: u64,
    field: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            output(
                out,
                c.get::<NativeInflationPrincipal>(id)?.flow.value(field)?,
            )
        })
    }
}

#[unsafe(no_mangle)]
/// Return a retained generic cash flow or, for as_leg=1, a one-flow CPI leg.
/// # Safety
/// Pointers must be aligned and valid, and outputs must not overlap. Context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_inflation_cashflow_convert(
    ctx: *mut Context,
    id: u64,
    as_leg: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let principal = c.get::<NativeInflationPrincipal>(id)?;
            let flow = principal.flow.erased();
            let result = if bool_value(as_leg)? {
                let leg: Leg = vec![flow];
                c.insert_with_metadata(leg, principal.settings)?
            } else {
                c.insert(flow)?
            };
            output(out, result)
        })
    }
}

#[unsafe(no_mangle)]
/// Return the principal payment date without reading inflation fixings.
/// # Safety
/// Pointers must be aligned and valid, and outputs must not overlap. Context
/// and handles belong to the calling thread. Serialize all context calls.
pub unsafe extern "C" fn itofin_inflation_cashflow_date(
    ctx: *mut Context,
    id: u64,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            output(
                out,
                c.get::<NativeInflationPrincipal>(id)?
                    .flow
                    .erased()
                    .date()
                    .serial_number(),
            )
        })
    }
}

#[cfg(test)]
#[path = "inflation_principal_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "inflation_principal_boundary_tests.rs"]
mod boundary_tests;
