//! The quasi-Newton approximation of the Hessian of the Lagrangian.
//!
//! The matrix is kept dense and refactored every iteration: the subproblem
//! needs its Cholesky factor, and the dimensions SLSQP is meant for keep the
//! cubic cost negligible next to the evaluations.
//!
//! - Powell, M. J. D. (1978), "A fast algorithm for nonlinearly constrained
//!   optimization calculations", Lecture Notes in Mathematics 630: the damped
//!   BFGS update.
//! - Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
//!   Springer, Section 18.3, Procedure 18.2: the same update in the form used
//!   here.

/// The curvature fraction below which the update is damped: the damped pair
/// keeps `s' r >= DAMPING s' B s`.
const DAMPING: f64 = 0.2;

/// A dense symmetric approximation `B`, stored row-major.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hessian {
    b: Vec<f64>,
    n: usize,
}

impl Hessian {
    /// The identity of dimension `n`, the starting approximation and the one a
    /// reset returns to.
    pub(crate) fn identity(n: usize) -> Self {
        let mut b = vec![0.0; n * n];
        for i in 0..n {
            b[i * n + i] = 1.0;
        }
        Self { b, n }
    }

    /// The product `B v`.
    pub(crate) fn times(&self, v: &[f64]) -> Vec<f64> {
        self.b
            .chunks_exact(self.n)
            .map(|row| row.iter().zip(v).map(|(b, v)| b * v).sum())
            .collect()
    }

    /// The lower-triangular factor `L` of `B = L L'`, row-major, or `None` when
    /// `B` is not numerically positive definite.
    pub(crate) fn cholesky(&self) -> Option<Vec<f64>> {
        let n = self.n;
        let mut l = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..=i {
                let dot: f64 = (0..j).map(|k| l[i * n + k] * l[j * n + k]).sum();
                let entry = self.b[i * n + j] - dot;
                if i == j {
                    if !(entry > 0.0 && entry.is_finite()) {
                        return None;
                    }
                    l[i * n + i] = entry.sqrt();
                } else {
                    l[i * n + j] = entry / l[j * n + j];
                }
            }
        }
        Some(l)
    }

    /// Applies the damped BFGS update for the step `s` and the change `y` in
    /// the gradient of the Lagrangian.
    ///
    /// When `s' y < 0.2 s' B s`, `y` is replaced by `r = theta y + (1 - theta)
    /// B s` with `theta = 0.8 s' B s / (s' B s - s' y)`, which lifts the
    /// curvature to `s' r = 0.2 s' B s > 0`. A positive definite `B` therefore
    /// stays positive definite whatever the sign of `s' y`. A step with
    /// `s' B s <= 0`, the zero step, leaves `B` unchanged.
    pub(crate) fn update(&mut self, s: &[f64], y: &[f64]) {
        let bs = self.times(s);
        let sbs = dot(s, &bs);
        if sbs <= 0.0 {
            return;
        }
        let sy = dot(s, y);
        let theta = if sy >= DAMPING * sbs {
            1.0
        } else {
            (1.0 - DAMPING) * sbs / (sbs - sy)
        };
        let r: Vec<f64> = y
            .iter()
            .zip(&bs)
            .map(|(y, bs)| theta * y + (1.0 - theta) * bs)
            .collect();
        let sr = dot(s, &r);
        let n = self.n;
        for i in 0..n {
            for j in 0..n {
                self.b[i * n + j] += r[i] * r[j] / sr - bs[i] * bs[j] / sbs;
            }
        }
    }
}

pub(crate) fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::Hessian;

    fn from_rows(rows: &[&[f64]]) -> Hessian {
        Hessian {
            b: rows.concat(),
            n: rows.len(),
        }
    }

    #[test]
    fn cholesky_reproduces_a_positive_definite_matrix() {
        let hessian = from_rows(&[&[4.0, 2.0, 0.0], &[2.0, 5.0, 1.0], &[0.0, 1.0, 3.0]]);
        let l = hessian.cholesky().expect("positive definite");
        for i in 0..3 {
            for j in 0..3 {
                let product: f64 = (0..3).map(|k| l[i * 3 + k] * l[j * 3 + k]).sum();
                assert!((product - hessian.b[i * 3 + j]).abs() <= 1e-14);
            }
        }
    }

    #[test]
    fn cholesky_rejects_an_indefinite_matrix() {
        assert!(from_rows(&[&[1.0, 2.0], &[2.0, 1.0]]).cholesky().is_none());
    }

    #[test]
    fn an_update_with_enough_curvature_satisfies_the_secant_condition() {
        let mut hessian = Hessian::identity(2);
        let (s, y) = ([1.0, 0.5], [2.0, 1.5]);
        hessian.update(&s, &y);
        let bs = hessian.times(&s);
        assert!(bs.iter().zip(&y).all(|(bs, y)| (bs - y).abs() <= 1e-14));
        assert!(hessian.cholesky().is_some());
    }

    /// `s' y = -1` against `s' B s = 1`: the plain BFGS update would set
    /// `B_00 = 1 - 1 + y_0^2 / s' y = -1`. The damping takes `theta = 0.4`,
    /// `r = (0.2, 0)`, and leaves `B = diag(0.2, 1)`.
    #[test]
    fn a_negative_curvature_step_keeps_the_update_positive_definite() {
        let mut hessian = Hessian::identity(2);
        hessian.update(&[1.0, 0.0], &[-1.0, 0.0]);
        assert!(hessian.cholesky().is_some(), "B = {:?}", hessian.b);
        let expected = [0.2, 0.0, 0.0, 1.0];
        assert!(
            hessian
                .b
                .iter()
                .zip(&expected)
                .all(|(b, e)| (b - e).abs() <= 1e-15)
        );
    }

    #[test]
    fn a_zero_step_leaves_the_approximation_unchanged() {
        let mut hessian = Hessian::identity(2);
        hessian.update(&[0.0, 0.0], &[1.0, 1.0]);
        assert_eq!(hessian, Hessian::identity(2));
    }
}
