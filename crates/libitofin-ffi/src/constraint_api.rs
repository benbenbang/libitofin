//! Reusable calibration constraints and per-call options.

use crate::boundary::*;
use libitofin::math::optimization::constraint::{
    BoundaryConstraint, CompositeConstraint, Constraint, NoConstraint, PositiveConstraint,
};

#[derive(Clone)]
pub(crate) enum ConstraintSpec {
    None,
    Positive,
    Boundary(f64, f64),
    Composite(Box<Self>, Box<Self>),
}

impl ConstraintSpec {
    fn build(&self) -> Box<dyn Constraint> {
        match self {
            Self::None => Box::new(NoConstraint),
            Self::Positive => Box::new(PositiveConstraint),
            Self::Boundary(low, high) => Box::new(BoundaryConstraint::new(*low, *high)),
            Self::Composite(left, right) => {
                Box::new(CompositeConstraint::new(left.build(), right.build()))
            }
        }
    }
}

#[repr(C)]
pub struct ItofinCalibrationOptions {
    pub constraint: u64,
    pub weights: *const f64,
    pub weights_len: usize,
    pub fix_parameters: *const u8,
    pub fix_parameters_len: usize,
}

pub(crate) struct CalibrationOptions {
    pub constraint: Option<Box<dyn Constraint>>,
    pub weights: Vec<f64>,
    pub fix_parameters: Vec<bool>,
}

/// # Safety
/// `options` and its arrays must obey the C caller contract for this call.
pub(crate) unsafe fn read_options(
    context: &Context,
    options: *const ItofinCalibrationOptions,
) -> BindingResult<CalibrationOptions> {
    if options.is_null() {
        return Ok(CalibrationOptions {
            constraint: None,
            weights: Vec::new(),
            fix_parameters: Vec::new(),
        });
    }
    check_ptr(options)?;
    let options = unsafe { &*options };
    let constraint = if options.constraint == 0 {
        None
    } else {
        Some(context.get::<ConstraintSpec>(options.constraint)?.build())
    };
    let weights = unsafe { input_slice(options.weights, options.weights_len)? }.to_vec();
    let fix_parameters =
        unsafe { input_slice(options.fix_parameters, options.fix_parameters_len)? }
            .iter()
            .map(|&flag| match flag {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(BindingError::invalid(
                    "fixed-parameter flags must be 0 or 1",
                )),
            })
            .collect::<BindingResult<Vec<_>>>()?;
    Ok(CalibrationOptions {
        constraint,
        weights,
        fix_parameters,
    })
}

#[unsafe(no_mangle)]
/// # Safety
/// Outputs and context must obey the C caller contract.
pub unsafe extern "C" fn itofin_no_constraint_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.insert(ConstraintSpec::None)?)
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Outputs and context must obey the C caller contract.
pub unsafe extern "C" fn itofin_positive_constraint_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.insert(ConstraintSpec::Positive)?)
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Bounds must be finite and ordered. Outputs and context obey the C caller contract.
pub unsafe extern "C" fn itofin_boundary_constraint_new(
    ctx: *mut Context,
    low: f64,
    high: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !low.is_finite() || !high.is_finite() || low > high {
                return Err(BindingError::invalid(
                    "constraint bounds must be finite and ordered",
                ));
            }
            output(out, c.insert(ConstraintSpec::Boundary(low, high))?)
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles, outputs and context must obey the C caller contract.
pub unsafe extern "C" fn itofin_composite_constraint_new(
    ctx: *mut Context,
    left: u64,
    right: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let left = c.get::<ConstraintSpec>(left)?;
            let right = c.get::<ConstraintSpec>(right)?;
            output(
                out,
                c.insert(ConstraintSpec::Composite(Box::new(left), Box::new(right)))?,
            )
        })
    }
}
