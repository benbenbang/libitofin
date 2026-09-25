use super::householder::Qr;
use super::lsei::{ldp, lsei, lsi};
use super::nnls::{Nnls, nnls};
use super::{Failure, MatRef, norm};

/// A 64-bit linear congruential generator returning uniform values in `[-1, 1)`.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1_u64 << 52) as f64 - 1.0
    }

    fn fill(&mut self, len: usize) -> Vec<f64> {
        (0..len).map(|_| self.next()).collect()
    }
}

fn mat(data: &[f64], rows: usize, cols: usize) -> MatRef<'_> {
    MatRef {
        data,
        rows,
        cols,
        stride: cols,
    }
}

/// `A' (b - A x)`.
fn gradient(a: MatRef<'_>, b: &[f64], x: &[f64]) -> Vec<f64> {
    let r: Vec<f64> = (0..a.rows)
        .map(|i| b[i] - (0..a.cols).map(|j| a.at(i, j) * x[j]).sum::<f64>())
        .collect();
    (0..a.cols)
        .map(|j| (0..a.rows).map(|i| a.at(i, j) * r[i]).sum())
        .collect()
}

/// The scale `||A||_F ||b||` of the dual vector, used for every KKT check.
fn dual_scale(a: MatRef<'_>, b: &[f64]) -> f64 {
    let frobenius = a.data.iter().map(|v| v * v).sum::<f64>().sqrt();
    frobenius * b.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// Asserts `x >= 0`, `w <= tol`, and `|w_j| <= tol` wherever `x_j > 0`, with
/// `tol = 1e-10 ||A||_F ||b||`.
fn assert_kkt(a: MatRef<'_>, b: &[f64], solution: &Nnls) {
    let tol = 1e-10 * dual_scale(a, b);
    let w = gradient(a, b, &solution.x);
    for (x, w) in solution.x.iter().zip(&w) {
        assert!(*x >= 0.0, "x = {x}");
        assert!(*w <= tol, "w = {w}");
        if *x > 0.0 {
            assert!(w.abs() <= tol, "x = {x}, w = {w}");
        }
    }
    let r: Vec<f64> = (0..a.rows)
        .map(|i| b[i] - (0..a.cols).map(|j| a.at(i, j) * solution.x[j]).sum::<f64>())
        .collect();
    let expected = r.iter().map(|v| v * v).sum::<f64>().sqrt();
    assert!((solution.residual_norm - expected).abs() <= 1e-12 * (1.0 + expected));
}

#[test]
fn qr_solves_full_rank_least_squares_with_orthogonal_residual() {
    let mut rng = Lcg(7);
    let (a, b) = (rng.fill(40), rng.fill(10));
    let a = mat(&a, 10, 4);
    let qr = Qr::factor(a);
    let (x, residual) = qr.solve(&b);
    assert_eq!(qr.rank(), 4);
    let tol = 1e-12 * dual_scale(a, &b);
    assert!(gradient(a, &b, &x).iter().all(|w| w.abs() <= tol));
    let expected = norm((0..10).map(|i| b[i] - (0..4).map(|j| a.at(i, j) * x[j]).sum::<f64>()));
    assert!((residual - expected).abs() <= 1e-12);
}

#[test]
fn qr_detects_a_dependent_column_and_returns_a_finite_basic_solution() {
    let mut rng = Lcg(11);
    let mut a = rng.fill(40);
    for i in 0..10 {
        a[i * 4 + 3] = a[i * 4] + a[i * 4 + 1];
    }
    let b = rng.fill(10);
    let a = mat(&a, 10, 4);
    let qr = Qr::factor(a);
    let (x, _) = qr.solve(&b);
    assert_eq!(qr.rank(), 3);
    assert!(x.iter().all(|v| v.is_finite()));
    assert_eq!(x.iter().filter(|v| **v == 0.0).count(), 1);
    let tol = 1e-12 * dual_scale(a, &b);
    assert!(gradient(a, &b, &x).iter().all(|w| w.abs() <= tol));
}

#[test]
fn qr_reads_through_the_row_stride() {
    let padded = [3.0, 99.0, 4.0, 99.0];
    let a = MatRef {
        data: &padded,
        rows: 2,
        cols: 1,
        stride: 2,
    };
    let (x, residual) = Qr::factor(a).solve(&[6.0, 8.0]);
    assert!((x[0] - 2.0).abs() <= 1e-15);
    assert!(residual.abs() <= 1e-15);
}

#[test]
fn nnls_satisfies_kkt_on_random_ten_by_four_systems() {
    for seed in 0..40 {
        let mut rng = Lcg(seed);
        let (a, b) = (rng.fill(40), rng.fill(10));
        let a = mat(&a, 10, 4);
        let solution = nnls(a, &b).expect("random system");
        assert_kkt(a, &b, &solution);
    }
}

#[test]
fn nnls_satisfies_kkt_when_a_is_rank_deficient() {
    for seed in 100..120 {
        let mut rng = Lcg(seed);
        let mut a = rng.fill(40);
        for i in 0..10 {
            a[i * 4 + 2] = a[i * 4] - 2.0 * a[i * 4 + 1];
        }
        let b = rng.fill(10);
        let a = mat(&a, 10, 4);
        let solution = nnls(a, &b).expect("rank-deficient system");
        assert_kkt(a, &b, &solution);
    }
}

#[test]
fn nnls_returns_the_unconstrained_solution_when_it_is_nonnegative() {
    let a = [1.0, 0.0, 0.0, 2.0, 1.0, 1.0];
    let x_true = [1.5, 0.25];
    let b: Vec<f64> = (0..3)
        .map(|i| a[i * 2] * x_true[0] + a[i * 2 + 1] * x_true[1])
        .collect();
    let solution = nnls(mat(&a, 3, 2), &b).expect("consistent system");
    assert!((solution.x[0] - 1.5).abs() <= 1e-14);
    assert!((solution.x[1] - 0.25).abs() <= 1e-14);
    assert!(solution.residual_norm <= 1e-14);
}

#[test]
fn qr_reflectors_are_orthogonal_and_the_pivots_permute_the_columns() {
    let mut rng = Lcg(3);
    let a = rng.fill(40);
    let qr = Qr::factor(mat(&a, 10, 4));
    let v = rng.fill(10);
    let mut w = v.clone();
    qr.apply_qt(&mut w);
    assert!((norm(w.iter().copied()) - norm(v.iter().copied())).abs() <= 1e-14);
    qr.apply_q(&mut w);
    assert!(v.iter().zip(&w).all(|(v, w)| (v - w).abs() <= 1e-14));
    let mut perm = qr.perm().to_vec();
    perm.sort_unstable();
    assert_eq!(perm, [0, 1, 2, 3]);
}

/// Column 1 is exactly three times column 0, so once column 1 is passive the
/// dual component of column 0 is rounding noise and may be positive. Without
/// the finite-termination safeguard column 0 enters, gets a zero trial
/// component, leaves at a zero step, and re-enters until the iteration cap.
#[test]
fn nnls_safeguard_terminates_on_proportional_columns() {
    let a = [1.0, 3.0, 1.0, 2.0, 6.0, 0.0, 3.0, 9.0, -1.0, 4.0, 12.0, 0.5];
    let b: Vec<f64> = (0..4)
        .map(|i| 0.7 * a[i * 3] + 0.2 * a[i * 3 + 2])
        .collect();
    let a = mat(&a, 4, 3);
    let solution = nnls(a, &b).expect("the safeguard terminates");
    assert_kkt(a, &b, &solution);
    assert!(solution.iterations <= 3);
    assert!(solution.residual_norm <= 1e-14);
}

fn assert_close(actual: &[f64], expected: &[f64], tol: f64) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() <= tol, "{actual:?} != {expected:?}");
    }
}

const NONE: MatRef<'static> = MatRef {
    data: &[],
    rows: 0,
    cols: 0,
    stride: 0,
};

#[test]
fn ldp_projects_the_origin_onto_the_active_half_plane() {
    let g = [1.0, 1.0, 1.0, -1.0];
    let solution = ldp(mat(&g, 2, 2), &[1.0, -5.0]).expect("feasible");
    assert_close(&solution.x, &[0.5, 0.5], 1e-14);
    assert_close(&solution.multipliers, &[0.5, 0.0], 1e-14);
}

#[test]
fn ldp_returns_the_origin_when_it_is_feasible() {
    let g = [1.0, 2.0, -3.0, 1.0];
    let solution = ldp(mat(&g, 2, 2), &[-1.0, 0.0]).expect("feasible");
    assert_close(&solution.x, &[0.0, 0.0], 0.0);
    assert_close(&solution.multipliers, &[0.0, 0.0], 0.0);
}

#[test]
fn ldp_reports_contradictory_inequalities_as_infeasible() {
    let g = [1.0, -1.0];
    assert_eq!(ldp(mat(&g, 2, 1), &[1.0, 0.0]), Err(Failure::Infeasible));
}

#[test]
fn lsi_moves_the_unconstrained_minimizer_onto_the_violated_constraint() {
    let e = [2.0, 0.0, 0.0, 1.0];
    let g = [-1.0, -1.0, 1.0, 0.0];
    let solution = lsi(mat(&e, 2, 2), &[4.0, 2.0], mat(&g, 2, 2), &[-3.0, 0.0]).expect("feasible");
    assert_close(&solution.x, &[1.8, 1.2], 1e-14);
    assert!((solution.residual_norm - 0.8_f64.sqrt()).abs() <= 1e-14);
    assert_close(&solution.multipliers, &[0.8, 0.0], 1e-14);
}

#[test]
fn lsi_keeps_the_unconstrained_minimizer_when_the_constraints_are_inactive() {
    let e = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let x_true = [1.0, -1.0];
    let f: Vec<f64> = (0..3).map(|i| e[i * 2] - e[i * 2 + 1]).collect();
    let g = [1.0, 0.0];
    let solution = lsi(mat(&e, 3, 2), &f, mat(&g, 1, 2), &[0.0]).expect("feasible");
    assert_close(&solution.x, &x_true, 1e-13);
    assert!(solution.residual_norm <= 1e-13);
}

#[test]
fn lsi_rejects_a_rank_deficient_objective_matrix() {
    let e = [1.0, 1.0, 2.0, 2.0];
    let result = lsi(mat(&e, 2, 2), &[1.0, 1.0], NONE, &[]);
    assert!(matches!(result, Err(Failure::RankDeficient)));
}

#[test]
fn lsei_solves_equality_and_active_inequality_together() {
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let c = [1.0, 1.0, 1.0];
    let g = [0.0, 0.0, -1.0];
    let solution = lsei(
        mat(&c, 1, 3),
        &[3.0],
        mat(&identity, 3, 3),
        &[1.0, 2.0, 3.0],
        mat(&g, 1, 3),
        &[-1.0],
    )
    .expect("feasible");
    assert_close(&solution.x, &[0.5, 1.5, 1.0], 1e-14);
    assert!((solution.residual_norm - 4.5_f64.sqrt()).abs() <= 1e-14);
    assert_close(&solution.multipliers, &[-0.5, 1.5], 1e-14);
}

#[test]
fn lsei_with_two_equalities_satisfies_them_at_the_active_bound() {
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let c = [1.0, 0.0, 1.0, 0.0, 2.0, -1.0];
    let g = [1.0, 0.0, 0.0];
    let solution = lsei(
        mat(&c, 2, 3),
        &[1.0, 0.0],
        mat(&identity, 3, 3),
        &[0.0, 0.0, 0.0],
        mat(&g, 1, 3),
        &[0.7],
    )
    .expect("feasible");
    let x = &solution.x;
    assert!((x[0] + x[2] - 1.0).abs() <= 1e-14);
    assert!((2.0 * x[1] - x[2]).abs() <= 1e-14);
    assert!(x[0] >= 0.7 - 1e-14);
    assert_close(x, &[0.7, 0.15, 0.3], 1e-14);
}

#[test]
fn lsei_rejects_dependent_equalities_even_when_consistent() {
    let c = [1.0, 1.0, 0.0, 2.0, 2.0, 0.0];
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let result = lsei(
        mat(&c, 2, 3),
        &[1.0, 2.0],
        mat(&identity, 3, 3),
        &[0.0; 3],
        NONE,
        &[],
    );
    assert!(matches!(result, Err(Failure::RankDeficient)));
}

#[test]
fn lsei_reports_an_inequality_that_contradicts_the_equalities_as_infeasible() {
    let c = [1.0, 0.0];
    let g = [-1.0, 0.0];
    let identity = [1.0, 0.0, 0.0, 1.0];
    let result = lsei(
        mat(&c, 1, 2),
        &[1.0],
        mat(&identity, 2, 2),
        &[0.0, 0.0],
        mat(&g, 1, 2),
        &[0.0],
    );
    assert!(matches!(result, Err(Failure::Infeasible)));
}
