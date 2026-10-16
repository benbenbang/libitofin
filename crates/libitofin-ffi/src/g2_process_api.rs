//! Immutable bare-factor G2 transitions with fixed-shape value results.
use crate::boundary::*;
use libitofin::math::{array::Array, matrix::Matrix};
use libitofin::processes::G2Process;
use libitofin::shared::{Shared, shared};
use libitofin::stochasticprocess::StochasticProcess;

/// Two state components in x, y order, passed and returned by value.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItofinG2Vector {
    pub values: [f64; 2],
}

/// Two-by-two matrix in row-major order: xx, xy, yx, yy.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItofinG2Matrix {
    pub values: [f64; 4],
}

fn vector(value: Array) -> BindingResult<ItofinG2Vector> {
    if value.iter().any(|v| !v.is_finite()) {
        return Err(BindingError::invalid("G2 vector result must be finite"));
    }
    Ok(ItofinG2Vector {
        values: [value[0], value[1]],
    })
}

fn matrix(value: Matrix) -> ItofinG2Matrix {
    ItofinG2Matrix {
        values: [value[(0, 0)], value[(0, 1)], value[(1, 0)], value[(1, 1)]],
    }
}

/// Create a bare-factor process. Signed reversions and zero volatility are valid.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_g2_process_new(
    ctx: *mut Context,
    a: f64,
    sigma: f64,
    b: f64,
    eta: f64,
    rho: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let p = G2Process::new(a, sigma, b, eta, rho)?;
            output(out, c.insert(shared(p))?)
        })
    }
}

/// Scalar kind: 0 size, 1 factors, 2 x0, 3 y0, 4 a, 5 sigma, 6 b, 7 eta, 8 rho.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_g2_process_scalar(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let p = c.get::<Shared<G2Process>>(process)?;
            let value = match kind {
                0 => p.size() as f64,
                1 => p.factors() as f64,
                2 => p.x0(),
                3 => p.y0(),
                4 => p.a(),
                5 => p.sigma(),
                6 => p.b(),
                7 => p.eta(),
                8 => p.rho(),
                _ => return Err(BindingError::invalid("unknown G2 scalar kind")),
            };
            output(out, value)
        })
    }
}

/// Vector kind: 0 initial values, 1 drift, 2 expectation, 3 apply, 4 evolve.
/// Drift uses t/state; expectation adds dt; apply uses state/change; evolve uses
/// t/state/dt/change, where change is two independent standard Gaussian draws.
/// Unused inputs are ignored. Apply requires finite inputs and finite sums.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_g2_process_vector(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    t: f64,
    state: ItofinG2Vector,
    dt: f64,
    change: ItofinG2Vector,
    out: *mut ItofinG2Vector,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let p = c.get::<Shared<G2Process>>(process)?;
            let x = Array::from(state.values);
            let dx = Array::from(change.values);
            let value = match kind {
                0 => p.initial_values()?,
                1 => p.drift(t, &x)?,
                2 => p.expectation(t, &x, dt)?,
                3 => {
                    if x.iter().chain(dx.iter()).any(|v| !v.is_finite()) {
                        return Err(BindingError::invalid("G2 apply requires finite inputs"));
                    }
                    p.apply(&x, &dx)
                }
                4 => p.evolve(t, &x, dt, &dx)?,
                _ => return Err(BindingError::invalid("unknown G2 vector kind")),
            };
            output(out, vector(value)?)
        })
    }
}

/// Matrix kind: 0 instantaneous diffusion, 1 exact step root, 2 covariance.
/// All use t/state; kinds 1 and 2 additionally use dt. Results are row-major.
/// Diffusion uses rho*eta in its lower-left entry, correcting QuantLib's
/// unequal-volatility defect. Step roots map independent standard normals.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_g2_process_matrix(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    t: f64,
    state: ItofinG2Vector,
    dt: f64,
    out: *mut ItofinG2Matrix,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let p = c.get::<Shared<G2Process>>(process)?;
            let x = Array::from(state.values);
            let value = match kind {
                0 => p.diffusion(t, &x)?,
                1 => p.std_deviation(t, &x, dt)?,
                2 => p.covariance(t, &x, dt)?,
                _ => return Err(BindingError::invalid("unknown G2 matrix kind")),
            };
            output(out, matrix(value))
        })
    }
}
