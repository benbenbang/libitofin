use super::householder::Qr;
use super::{MatRef, norm};

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
