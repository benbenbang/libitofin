//! The quadratic subproblem of one SQP iteration, solved as a constrained
//! least-squares problem.
//!
//! At `x`, with the gradient `g`, the constraint values `c`, their Jacobian `A`
//! and the Hessian approximation `B = L L'`, the step `d` solves
//!
//! `min d' B d / 2 + g' d` subject to `a_j' d + c_j = 0` (equalities),
//! `a_j' d + c_j >= 0` (inequalities) and `l - x <= d <= u - x`.
//!
//! Since `d' B d / 2 + g' d = ||L' d + L^-1 g||^2 / 2 - ||L^-1 g||^2 / 2`, this
//! is LSEI with `E = L'` and `f = -L^-1 g` (Kraft 1988).
//!
//! When the linearized constraints are incompatible, or the equality rows are
//! rank deficient, the subproblem is relaxed with one more variable
//! `delta` in `[0, 1]` (Kraft 1988): every equality and every violated
//! inequality becomes `a_j' d + (1 - delta) c_j` against zero, and the
//! objective gains `RELAXATION_WEIGHT delta^2 / 2`. The relaxed problem is
//! always feasible, at `d = 0` and `delta = 1`, because the iterate lies inside
//! its bounds. Its equalities enter as pairs of opposing inequalities, which
//! LDP accepts even when they are linearly dependent, so a rank-deficient but
//! consistent Jacobian yields a step with `delta` near zero.

use super::hessian::dot;
use crate::lsq::lsei::{lsei, lsi};
use crate::lsq::{Failure, MatRef};
use crate::objective::ConstraintKind;

/// The weight of `delta^2 / 2` in the relaxed objective. Large, so that
/// `delta` stays near zero whenever the linearization is compatible and the
/// step recovers almost all of the Newton decrease in the constraints.
const RELAXATION_WEIGHT: f64 = 1e6;

/// The first-order model of the problem at the current iterate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Linearization<'a> {
    /// The objective gradient `g`.
    pub(crate) gradient: &'a [f64],
    /// The constraint values `c`.
    pub(crate) values: &'a [f64],
    /// The constraint Jacobian `A`, one row per constraint, row-major.
    pub(crate) jacobian: &'a [f64],
    /// The form of each constraint.
    pub(crate) kinds: &'a [ConstraintKind],
    /// The lower step bounds `l - x`, `-inf` where open.
    pub(crate) lower: &'a [f64],
    /// The upper step bounds `u - x`, `+inf` where open.
    pub(crate) upper: &'a [f64],
}

/// A solution of the subproblem.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    /// The search direction `d`.
    pub(crate) d: Vec<f64>,
    /// One multiplier per constraint, in constraint order, satisfying
    /// `B d + g = sum_j multipliers_j a_j` up to the bound multipliers.
    /// Inequality multipliers are nonnegative.
    pub(crate) multipliers: Vec<f64>,
    /// The relaxation `delta`: zero when the subproblem was solved as stated,
    /// one when no step reduces the linearized violation at all.
    pub(crate) relaxation: f64,
}

/// A row-major matrix under construction, one inequality or equality row at a
/// time.
struct Rows {
    data: Vec<f64>,
    rhs: Vec<f64>,
    cols: usize,
}

impl Rows {
    fn new(cols: usize) -> Self {
        Self {
            data: Vec::new(),
            rhs: Vec::new(),
            cols,
        }
    }

    fn push(&mut self, row: impl IntoIterator<Item = f64>, rhs: f64) {
        self.data.extend(row);
        self.data
            .resize(self.rhs.len() * self.cols + self.cols, 0.0);
        self.rhs.push(rhs);
    }

    fn view(&self) -> MatRef<'_> {
        MatRef {
            data: &self.data,
            rows: self.rhs.len(),
            cols: self.cols,
            stride: self.cols,
        }
    }

    /// The bound rows `d_i >= lower_i` and `-d_i >= -upper_i` on the finite
    /// sides.
    fn push_bounds(&mut self, lower: &[f64], upper: &[f64]) {
        for (i, (&lower, &upper)) in lower.iter().zip(upper).enumerate() {
            if lower.is_finite() {
                self.push(unit(i, 1.0), lower);
            }
            if upper.is_finite() {
                self.push(unit(i, -1.0), -upper);
            }
        }
    }
}

fn unit(i: usize, sign: f64) -> impl Iterator<Item = f64> {
    (0..=i).map(move |k| if k == i { sign } else { 0.0 })
}

/// Solves the subproblem for the Cholesky factor `l` of `B`, lower-triangular
/// and row-major.
///
/// # Errors
///
/// The [`Failure`] of the relaxed problem, reached only by the NNLS iteration
/// cap or rounding severe enough to defeat its guaranteed feasibility.
pub(crate) fn solve(l: &[f64], model: &Linearization<'_>) -> Result<Step, Failure> {
    let n = model.gradient.len();
    let mut w = model.gradient.to_vec();
    for i in 0..n {
        w[i] = (w[i] - dot(&l[i * n..i * n + i], &w[..i])) / l[i * n + i];
    }
    let f: Vec<f64> = w.iter().map(|w| -w).collect();
    exact(l, &f, model).or_else(|_| relaxed(l, &f, model))
}

fn row<'a>(model: &Linearization<'a>, j: usize) -> &'a [f64] {
    let n = model.gradient.len();
    &model.jacobian[j * n..(j + 1) * n]
}

/// The subproblem as stated, through LSEI.
fn exact(l: &[f64], f: &[f64], model: &Linearization<'_>) -> Result<Step, Failure> {
    let n = f.len();
    let e: Vec<f64> = (0..n * n).map(|k| l[(k % n) * n + k / n]).collect();
    let (mut equalities, mut inequalities) = (Rows::new(n), Rows::new(n));
    for (j, (&kind, &c)) in model.kinds.iter().zip(model.values).enumerate() {
        let target = match kind {
            ConstraintKind::Eq => &mut equalities,
            ConstraintKind::Ineq => &mut inequalities,
        };
        target.push(row(model, j).iter().copied(), -c);
    }
    inequalities.push_bounds(model.lower, model.upper);
    let solution = lsei(
        equalities.view(),
        &equalities.rhs,
        MatRef {
            data: &e,
            rows: n,
            cols: n,
            stride: n,
        },
        f,
        inequalities.view(),
        &inequalities.rhs,
    )?;
    let (mut eq, mut ineq) = (0, equalities.rhs.len());
    let multipliers = model
        .kinds
        .iter()
        .map(|kind| {
            let slot = match kind {
                ConstraintKind::Eq => &mut eq,
                ConstraintKind::Ineq => &mut ineq,
            };
            *slot += 1;
            solution.multipliers[*slot - 1]
        })
        .collect();
    Ok(Step {
        d: solution.x,
        multipliers,
        relaxation: 0.0,
    })
}

/// The relaxed subproblem in `(d, delta)`, through LSI.
fn relaxed(l: &[f64], f: &[f64], model: &Linearization<'_>) -> Result<Step, Failure> {
    let n = f.len();
    let cols = n + 1;
    let mut e = vec![0.0; cols * cols];
    for i in 0..n {
        for j in i..n {
            e[i * cols + j] = l[j * n + i];
        }
    }
    e[cols * cols - 1] = RELAXATION_WEIGHT.sqrt();
    let mut rhs = f.to_vec();
    rhs.push(0.0);
    let mut g = Rows::new(cols);
    for (j, (&kind, &c)) in model.kinds.iter().zip(model.values).enumerate() {
        let a = row(model, j).iter().copied();
        match kind {
            ConstraintKind::Eq => {
                g.push(a.clone().chain([-c]), -c);
                g.push(a.map(|a| -a).chain([c]), c);
            }
            ConstraintKind::Ineq => g.push(a.chain([if c < 0.0 { -c } else { 0.0 }]), -c),
        }
    }
    g.push_bounds(model.lower, model.upper);
    g.push(unit(n, 1.0), 0.0);
    g.push(unit(n, -1.0), -1.0);
    let solution = lsi(
        MatRef {
            data: &e,
            rows: cols,
            cols,
            stride: cols,
        },
        &rhs,
        g.view(),
        &g.rhs,
    )?;
    let mut next = 0;
    let multipliers = model
        .kinds
        .iter()
        .map(|kind| {
            let lambda = &solution.multipliers[next..];
            match kind {
                ConstraintKind::Eq => {
                    next += 2;
                    lambda[0] - lambda[1]
                }
                ConstraintKind::Ineq => {
                    next += 1;
                    lambda[0]
                }
            }
        })
        .collect();
    let mut d = solution.x;
    let relaxation = d.pop().unwrap_or_default().clamp(0.0, 1.0);
    Ok(Step {
        d,
        multipliers,
        relaxation,
    })
}

#[cfg(test)]
mod tests {
    use super::{Linearization, solve};
    use crate::objective::ConstraintKind::{self, Eq, Ineq};

    const IDENTITY: [f64; 4] = [1.0, 0.0, 0.0, 1.0];
    const OPEN: [f64; 2] = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    const OPEN_ABOVE: [f64; 2] = [f64::INFINITY, f64::INFINITY];

    fn model<'a>(
        gradient: &'a [f64],
        values: &'a [f64],
        jacobian: &'a [f64],
        kinds: &'a [ConstraintKind],
    ) -> Linearization<'a> {
        Linearization {
            gradient,
            values,
            jacobian,
            kinds,
            lower: &OPEN,
            upper: &OPEN_ABOVE,
        }
    }

    fn close(actual: &[f64], expected: &[f64], tol: f64) -> bool {
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| (a - e).abs() <= tol)
    }

    #[test]
    fn without_constraints_the_step_is_the_newton_step() {
        let l = [2.0, 0.0, 1.0, 1.0];
        let step = solve(&l, &model(&[2.0, -1.0], &[], &[], &[])).expect("solvable");
        assert!(close(&step.d, &[-1.5, 2.0], 1e-14), "{:?}", step.d);
        assert_eq!(step.relaxation, 0.0);
    }

    #[test]
    fn an_equality_and_an_active_inequality_carry_their_multipliers() {
        let jacobian = [1.0, -2.0, 1.0, 1.0];
        let step = solve(
            &IDENTITY,
            &model(&[-1.0, -2.5], &[2.0, -1.0], &jacobian, &[Ineq, Eq]),
        )
        .expect("solvable");
        assert!(close(&step.d, &[0.0, 1.0], 1e-14), "{:?}", step.d);
        assert!(
            close(&step.multipliers, &[1.0 / 6.0, -7.0 / 6.0], 1e-14),
            "{:?}",
            step.multipliers
        );
        assert_eq!(step.relaxation, 0.0);
    }

    #[test]
    fn a_bound_caps_the_step() {
        let mut linearization = model(&[-4.0, 0.0], &[], &[], &[]);
        linearization.upper = &[1.0, f64::INFINITY];
        let step = solve(&IDENTITY, &linearization).expect("solvable");
        assert!(close(&step.d, &[1.0, 0.0], 1e-14), "{:?}", step.d);
    }

    /// The second row is twice the first, so LSEI rejects the equalities and
    /// the relaxation solves them. The relaxed optimum has
    /// `delta = 1 / (1 + 2 rho)` and `d_i = (1 - delta) / 2`.
    #[test]
    fn dependent_equalities_are_solved_through_the_relaxation() {
        let jacobian = [1.0, 1.0, 2.0, 2.0];
        let step = solve(
            &IDENTITY,
            &model(&[0.0, 0.0], &[-1.0, -2.0], &jacobian, &[Eq, Eq]),
        )
        .expect("solvable");
        let delta = 1.0 / (1.0 + 2e6);
        assert!(
            (step.relaxation - delta).abs() <= 1e-12,
            "{}",
            step.relaxation
        );
        assert!(close(&step.d, &[0.5, 0.5], 1e-6), "{:?}", step.d);
        let combined = step.multipliers[0] + 2.0 * step.multipliers[1];
        assert!(
            (combined - step.d[0]).abs() <= 1e-10,
            "{:?}",
            step.multipliers
        );
    }

    #[test]
    fn contradictory_inequalities_relax_all_the_way() {
        let jacobian = [1.0, 0.0, -1.0, 0.0];
        let step = solve(
            &IDENTITY,
            &model(&[0.0, 0.0], &[-0.5, -0.5], &jacobian, &[Ineq, Ineq]),
        )
        .expect("the relaxed problem is always feasible");
        assert!(
            (step.relaxation - 1.0).abs() <= 1e-12,
            "{}",
            step.relaxation
        );
        assert!(close(&step.d, &[0.0, 0.0], 1e-12), "{:?}", step.d);
    }
}
