//! Runge-Kutta ODE integration ported from `ql/math/ode/adaptiverungekutta.hpp`.
//!
//! Runge-Kutta method with adaptive stepsize (Cash-Karp embedded pair) as
//! described in Numerical Recipes in C, chapter 16.2.

use std::ops::{Add, Mul};

use crate::errors::QlResult;
use crate::fail;
use crate::types::Real;

/// A scalar the ODE state is made of.
///
/// QuantLib instantiates `AdaptiveRungeKutta` for `Real` and
/// `std::complex<Real>`; here any copyable type closed under addition and
/// scaling by a [`Real`] qualifies, with [`abs`](OdeScalar::abs) supplying the
/// magnitude used for step-size error control.
pub trait OdeScalar: Copy + Add<Output = Self> + Mul<Real, Output = Self> {
    /// The magnitude used in the error estimate.
    fn abs(self) -> Real;
}

impl OdeScalar for Real {
    fn abs(self) -> Real {
        Real::abs(self)
    }
}

const MAX_STEPS: usize = 10_000;
const TINY: Real = 1.0e-30;
const SAFETY: Real = 0.9;
const P_GROW: Real = -0.2;
const P_SHRINK: Real = -0.25;
const ERRCON: Real = 1.89e-4;

const A2: Real = 0.2;
const A3: Real = 0.3;
const A4: Real = 0.6;
const A5: Real = 1.0;
const A6: Real = 0.875;
const B21: Real = 0.2;
const B31: Real = 3.0 / 40.0;
const B32: Real = 9.0 / 40.0;
const B41: Real = 0.3;
const B42: Real = -0.9;
const B43: Real = 1.2;
const B51: Real = -11.0 / 54.0;
const B52: Real = 2.5;
const B53: Real = -70.0 / 27.0;
const B54: Real = 35.0 / 27.0;
const B61: Real = 1631.0 / 55296.0;
const B62: Real = 175.0 / 512.0;
const B63: Real = 575.0 / 13824.0;
const B64: Real = 44275.0 / 110592.0;
const B65: Real = 253.0 / 4096.0;
const C1: Real = 37.0 / 378.0;
const C3: Real = 250.0 / 621.0;
const C4: Real = 125.0 / 594.0;
const C6: Real = 512.0 / 1771.0;
const DC1: Real = C1 - 2825.0 / 27648.0;
const DC3: Real = C3 - 18575.0 / 48384.0;
const DC4: Real = C4 - 13525.0 / 55296.0;
const DC5: Real = -277.0 / 14336.0;
const DC6: Real = C6 - 0.25;

/// Runge-Kutta integrator with adaptive stepsize.
///
/// Port of `QuantLib::AdaptiveRungeKutta`.
///
/// # Examples
///
/// ```
/// use itofin::math::ode::AdaptiveRungeKutta;
///
/// let rk = AdaptiveRungeKutta::default();
/// let y = rk.solve_1d(|_x, y: f64| y, 1.0, 0.0, 1.0)?;
/// assert!((y - 1.0_f64.exp()).abs() < 1e-5);
/// # Ok::<(), itofin::errors::QlError>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct AdaptiveRungeKutta {
    eps: Real,
    h1: Real,
    hmin: Real,
}

impl Default for AdaptiveRungeKutta {
    /// The QuantLib defaults: `eps = 1e-6`, `h1 = 1e-4`, `hmin = 0`.
    fn default() -> Self {
        AdaptiveRungeKutta::new(1.0e-6, 1.0e-4, 0.0)
    }
}

impl AdaptiveRungeKutta {
    /// An integrator with prescribed error `eps`, start step size `h1` and
    /// smallest allowed step size `hmin`.
    pub fn new(eps: Real, h1: Real, hmin: Real) -> Self {
        AdaptiveRungeKutta { eps, h1, hmin }
    }

    /// Integrates the system `f'(x) = ode(x, f(x))` from `x1` to `x2` with
    /// the initial value condition `f(x1) = y1`.
    ///
    /// # Errors
    ///
    /// Returns an error if the adaptive step size underflows below `hmin` (or
    /// to zero) or the step count exceeds the internal limit, as `QL_FAIL`
    /// does in QuantLib.
    pub fn solve<T, F>(&self, ode: F, y1: &[T], x1: Real, x2: Real) -> QlResult<Vec<T>>
    where
        T: OdeScalar,
        F: Fn(Real, &[T]) -> Vec<T>,
    {
        let n = y1.len();
        let mut y = y1.to_vec();
        let mut y_scale = vec![0.0; n];
        let mut x = x1;
        let mut h = self.h1 * if x1 <= x2 { 1.0 } else { -1.0 };

        for _ in 0..MAX_STEPS {
            let dydx = ode(x, &y);
            for (scale, (&yi, &di)) in y_scale.iter_mut().zip(y.iter().zip(&dydx)) {
                *scale = yi.abs() + (di * h).abs() + TINY;
            }
            if (x + h - x2) * (x + h - x1) > 0.0 {
                h = x2 - x;
            }
            let h_next = self.rkqs(&mut y, &dydx, &mut x, h, &y_scale, &ode)?;

            if (x - x2) * (x2 - x1) >= 0.0 {
                return Ok(y);
            }

            if h_next.abs() <= self.hmin {
                fail!(
                    "step size ({h_next}) too small ({} min) in AdaptiveRungeKutta",
                    self.hmin
                );
            }
            h = h_next;
        }
        fail!("too many steps ({MAX_STEPS}) in AdaptiveRungeKutta")
    }

    /// Integrates the scalar equation `f'(x) = ode(x, f(x))` from `x1` to
    /// `x2` with the initial value condition `f(x1) = y1`.
    ///
    /// # Errors
    ///
    /// Propagates the failures of [`solve`](AdaptiveRungeKutta::solve).
    pub fn solve_1d<T, F>(&self, ode: F, y1: T, x1: Real, x2: Real) -> QlResult<T>
    where
        T: OdeScalar,
        F: Fn(Real, T) -> T,
    {
        Ok(self.solve(|x, y: &[T]| vec![ode(x, y[0])], &[y1], x1, x2)?[0])
    }

    fn rkqs<T, F>(
        &self,
        y: &mut [T],
        dydx: &[T],
        x: &mut Real,
        h_try: Real,
        y_scale: &[Real],
        derivs: &F,
    ) -> QlResult<Real>
    where
        T: OdeScalar,
        F: Fn(Real, &[T]) -> Vec<T>,
    {
        let mut h = h_try;
        loop {
            let (y_temp, y_err) = rkck(y, dydx, *x, h, derivs);
            let mut err_max: Real = 0.0;
            for (&err, &scale) in y_err.iter().zip(y_scale) {
                err_max = err_max.max(err.abs() / scale);
            }
            err_max /= self.eps;
            if err_max > 1.0 {
                let h_temp1 = SAFETY * h * err_max.powf(P_SHRINK);
                let h_temp2 = h / 10.0;
                h = if h >= 0.0 {
                    h_temp1.max(h_temp2)
                } else {
                    h_temp1.min(h_temp2)
                };
                let x_new = *x + h;
                if x_new == *x {
                    fail!("stepsize underflow ({h} at x = {x}) in AdaptiveRungeKutta::rkqs");
                }
            } else {
                let h_next = if err_max > ERRCON {
                    SAFETY * h * err_max.powf(P_GROW)
                } else {
                    5.0 * h
                };
                *x += h;
                y.copy_from_slice(&y_temp);
                return Ok(h_next);
            }
        }
    }
}

fn rkck<T, F>(y: &[T], dydx: &[T], x: Real, h: Real, derivs: &F) -> (Vec<T>, Vec<T>)
where
    T: OdeScalar,
    F: Fn(Real, &[T]) -> Vec<T>,
{
    let n = y.len();

    let mut y_temp: Vec<T> = (0..n).map(|i| y[i] + dydx[i] * (B21 * h)).collect();
    let ak2 = derivs(x + A2 * h, &y_temp);

    for i in 0..n {
        y_temp[i] = y[i] + (dydx[i] * B31 + ak2[i] * B32) * h;
    }
    let ak3 = derivs(x + A3 * h, &y_temp);

    for i in 0..n {
        y_temp[i] = y[i] + (dydx[i] * B41 + ak2[i] * B42 + ak3[i] * B43) * h;
    }
    let ak4 = derivs(x + A4 * h, &y_temp);

    for i in 0..n {
        y_temp[i] = y[i] + (dydx[i] * B51 + ak2[i] * B52 + ak3[i] * B53 + ak4[i] * B54) * h;
    }
    let ak5 = derivs(x + A5 * h, &y_temp);

    for i in 0..n {
        y_temp[i] =
            y[i] + (dydx[i] * B61 + ak2[i] * B62 + ak3[i] * B63 + ak4[i] * B64 + ak5[i] * B65) * h;
    }
    let ak6 = derivs(x + A6 * h, &y_temp);

    let y_out = (0..n)
        .map(|i| y[i] + (dydx[i] * C1 + ak3[i] * C3 + ak4[i] * C4 + ak6[i] * C6) * h)
        .collect();
    let y_err = (0..n)
        .map(|i| (dydx[i] * DC1 + ak3[i] * DC3 + ak4[i] * DC4 + ak5[i] * DC5 + ak6[i] * DC6) * h)
        .collect();
    (y_out, y_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct Complex {
        re: Real,
        im: Real,
    }

    impl Complex {
        fn new(re: Real, im: Real) -> Self {
            Complex { re, im }
        }
    }

    impl Add for Complex {
        type Output = Complex;

        fn add(self, rhs: Complex) -> Complex {
            Complex::new(self.re + rhs.re, self.im + rhs.im)
        }
    }

    impl Mul<Real> for Complex {
        type Output = Complex;

        fn mul(self, rhs: Real) -> Complex {
            Complex::new(self.re * rhs, self.im * rhs)
        }
    }

    impl OdeScalar for Complex {
        fn abs(self) -> Real {
            self.re.hypot(self.im)
        }
    }

    fn dist(a: Complex, b: Complex) -> Real {
        (a.re - b.re).hypot(a.im - b.im)
    }

    #[test]
    fn adaptive_runge_kutta_matches_exact_solutions() {
        let rk_real = AdaptiveRungeKutta::new(1e-12, 1e-4, 0.0);
        let rk_complex = AdaptiveRungeKutta::new(1e-12, 1e-4, 0.0);
        let (tol1, tol2, tol3, tol4) = (5e-10, 2e-12, 2e-12, 2e-12);

        let ode1 = |_x: Real, y: Real| y;
        let ode2 = |_x: Real, y: Complex| Complex::new(-y.im, y.re);
        let ode3 = |_x: Real, y: &[Real]| vec![y[1], -y[0]];
        let ode4 = |_x: Real, y: &[Complex]| vec![y[1], y[0] * -1.0];

        let y10 = 1.0;
        let y20 = Complex::new(0.0, 1.0);
        let y30 = [0.0, 1.0];
        let y40 = [Complex::new(1.0, 0.0), Complex::new(0.0, 1.0)];

        let mut x: Real = 0.0;
        let mut y1 = y10;
        let mut y2 = y20;
        let mut y3 = y30.to_vec();
        let mut y4 = y40.to_vec();

        while x < 5.0 {
            let exact1 = x.exp();
            let exact2 = Complex::new(-x.sin(), x.cos());
            let exact3 = x.sin();
            let exact4 = Complex::new(x.cos(), x.sin());

            assert!(
                (exact1 - y1).abs() <= tol1,
                "ode1 at x={x}: got {y1}, want {exact1}"
            );
            assert!(
                dist(exact2, y2) <= tol2,
                "ode2 at x={x}: got {y2:?}, want {exact2:?}"
            );
            assert!(
                (exact3 - y3[0]).abs() <= tol3,
                "ode3 at x={x}: got {}, want {exact3}",
                y3[0]
            );
            assert!(
                dist(exact4, y4[0]) <= tol4,
                "ode4 at x={x}: got {:?}, want {exact4:?}",
                y4[0]
            );

            x += 0.01;
            y1 = rk_real.solve_1d(ode1, y10, 0.0, x).unwrap();
            y2 = rk_complex.solve_1d(ode2, y20, 0.0, x).unwrap();
            y3 = rk_real.solve(ode3, &y30, 0.0, x).unwrap();
            y4 = rk_complex.solve(ode4, &y40, 0.0, x).unwrap();
        }
    }

    #[test]
    fn integrates_backwards() {
        let rk = AdaptiveRungeKutta::new(1e-12, 1e-4, 0.0);
        let y = rk.solve_1d(|_x, y: Real| y, 1.0, 0.0, -1.0).unwrap();
        assert!((y - (-1.0_f64).exp()).abs() <= 1e-10);
    }
}
