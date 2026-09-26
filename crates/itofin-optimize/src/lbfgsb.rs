//! Limited-memory BFGS matrices for bound-constrained optimization.
//!
//! The compact Hessian is `B = theta I - W M W'`, with
//! `W = [Y, theta S]` and `M` the inverse of the small block matrix
//! `[-D, L'; L, theta S'S]`. The correction pairs are stored oldest first.
//!
//! - Byrd, Lu, Nocedal and Zhu (1995), "A Limited Memory Algorithm for Bound
//!   Constrained Optimization", SIAM J. Sci. Comput. 16(5), Section 3.
//! - Byrd, Nocedal and Schnabel (1994), "Representations of Quasi-Newton
//!   Matrices and their use in Limited Memory Methods", Math. Program. 63.

use std::collections::VecDeque;

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(ai, bi)| ai * bi).sum()
}

struct Pair {
    s: Vec<f64>,
    y: Vec<f64>,
    sy: f64,
}

pub(crate) struct Compact {
    n: usize,
    maxcor: usize,
    pairs: VecDeque<Pair>,
    theta: f64,
}

impl Compact {
    pub(crate) fn new(n: usize, maxcor: usize) -> Self {
        assert!(maxcor > 0);
        Self {
            n,
            maxcor,
            pairs: VecDeque::with_capacity(maxcor),
            theta: 1.0,
        }
    }

    pub(crate) fn update(&mut self, s: Vec<f64>, y: Vec<f64>) -> bool {
        assert_eq!(s.len(), self.n);
        assert_eq!(y.len(), self.n);
        let sy = dot(&s, &y);
        let yy = dot(&y, &y);
        if !(sy.is_finite() && yy.is_finite() && sy > f64::EPSILON * yy) {
            return false;
        }
        let theta = yy / sy;
        if !theta.is_finite() || theta <= 0.0 {
            return false;
        }
        if self.pairs.len() == self.maxcor {
            self.pairs.pop_front();
        }
        self.theta = theta;
        self.pairs.push_back(Pair { s, y, sy });
        true
    }

    pub(crate) fn inverse_times(&self, v: &[f64]) -> Vec<f64> {
        assert_eq!(v.len(), self.n);
        let mut q = v.to_vec();
        let mut alpha = vec![0.0; self.pairs.len()];
        for (i, pair) in self.pairs.iter().enumerate().rev() {
            alpha[i] = dot(&pair.s, &q) / pair.sy;
            for (qi, yi) in q.iter_mut().zip(&pair.y) {
                *qi -= alpha[i] * yi;
            }
        }
        for qi in &mut q {
            *qi /= self.theta;
        }
        for (pair, alpha_i) in self.pairs.iter().zip(alpha) {
            let beta = dot(&pair.y, &q) / pair.sy;
            for (qi, si) in q.iter_mut().zip(&pair.s) {
                *qi += (alpha_i - beta) * si;
            }
        }
        q
    }

    pub(crate) fn times(&self, v: &[f64]) -> Vec<f64> {
        assert_eq!(v.len(), self.n);
        let count = self.pairs.len();
        if count == 0 {
            return v.iter().map(|vi| self.theta * vi).collect();
        }
        let size = 2 * count;
        let mut block = vec![vec![0.0; size]; size];
        let mut rhs = vec![0.0; size];
        for (i, pi) in self.pairs.iter().enumerate() {
            block[i][i] = -pi.sy;
            rhs[i] = dot(&pi.y, v);
            rhs[count + i] = self.theta * dot(&pi.s, v);
            for (j, pj) in self.pairs.iter().enumerate() {
                block[count + i][count + j] = self.theta * dot(&pi.s, &pj.s);
                if i > j {
                    let lij = dot(&pi.s, &pj.y);
                    block[count + i][j] = lij;
                    block[j][count + i] = lij;
                }
            }
        }
        let weights = solve(block, rhs);
        let mut result: Vec<f64> = v.iter().map(|vi| self.theta * vi).collect();
        for (i, pair) in self.pairs.iter().enumerate() {
            for (ri, (&yi, &si)) in result.iter_mut().zip(pair.y.iter().zip(&pair.s)) {
                *ri -= weights[i] * yi + self.theta * weights[count + i] * si;
            }
        }
        result
    }
}

fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for k in 0..n {
        let pivot = (k..n)
            .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
            .expect("nonempty pivot range");
        a.swap(k, pivot);
        b.swap(k, pivot);
        let diagonal = a[k][k];
        let (upper, lower) = a.split_at_mut(k + 1);
        for (offset, row) in lower.iter_mut().enumerate() {
            let i = k + 1 + offset;
            let factor = row[k] / diagonal;
            for (entry, pivot_entry) in row[k + 1..].iter_mut().zip(&upper[k][k + 1..]) {
                *entry -= factor * pivot_entry;
            }
            b[i] -= factor * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - dot(&a[i][i + 1..], &x[i + 1..])) / a[i][i];
    }
    x
}

#[cfg(test)]
mod tests {
    use super::{Compact, dot};

    fn dense_update(b: &mut [Vec<f64>], s: &[f64], y: &[f64]) {
        let bs: Vec<f64> = b.iter().map(|row| dot(row, s)).collect();
        let sbs = dot(s, &bs);
        let sy = dot(s, y);
        for i in 0..b.len() {
            for j in 0..b.len() {
                b[i][j] += y[i] * y[j] / sy - bs[i] * bs[j] / sbs;
            }
        }
    }

    #[test]
    fn compact_product_agrees_with_explicit_bfgs() {
        let mut compact = Compact::new(5, 3);
        let pairs = [
            ([1.0, 0.0, 0.5, 0.0, 0.0], [2.0, 0.1, 0.5, 0.0, 0.0]),
            ([0.0, 1.0, 0.0, 0.5, 0.0], [0.1, 3.0, 0.0, 0.5, 0.0]),
            ([0.0, 0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.1, 0.0, 4.0]),
        ];
        for (s, y) in pairs {
            assert!(compact.update(s.to_vec(), y.to_vec()));
        }
        let theta = compact.theta;
        let mut dense = vec![vec![0.0; 5]; 5];
        for (i, row) in dense.iter_mut().enumerate() {
            row[i] = theta;
        }
        for (s, y) in pairs {
            dense_update(&mut dense, &s, &y);
        }
        for v in [[1.0, -2.0, 3.0, -4.0, 5.0], [0.1, 0.2, 0.3, 0.4, 0.5]] {
            let actual = compact.times(&v);
            let expected: Vec<f64> = dense.iter().map(|row| dot(row, &v)).collect();
            for (a, e) in actual.iter().zip(expected) {
                assert!((a - e).abs() < 1e-12, "{a} vs {e}");
            }
            let recovered = compact.inverse_times(&actual);
            for (r, e) in recovered.iter().zip(v) {
                assert!((r - e).abs() < 1e-12, "{r} vs {e}");
            }
        }
    }

    #[test]
    fn memory_discards_the_oldest_pair() {
        let mut compact = Compact::new(2, 1);
        assert!(compact.update(vec![1.0, 0.0], vec![2.0, 0.0]));
        assert!(compact.update(vec![0.0, 1.0], vec![0.0, 3.0]));
        assert_eq!(compact.pairs.len(), 1);
        assert_eq!(compact.pairs[0].s, vec![0.0, 1.0]);
    }
}
