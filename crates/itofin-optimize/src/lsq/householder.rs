//! Householder QR with column pivoting, `A P = Q R`.

use super::{MatRef, norm};

/// One elementary reflector `H = I + u u' / b`, built by Algorithm H1 to map a
/// column segment onto its first coordinate. `u[0]` is the pivot component and
/// `b = s * u[0]` with `s` the new diagonal entry, so `b < 0`.
#[derive(Debug, Clone)]
struct Reflector {
    u: Vec<f64>,
    b: f64,
}

impl Reflector {
    /// Algorithm H2: overwrites the entries `v[offset + i * stride]`,
    /// `i < u.len()`, with their image under `H`.
    fn apply(&self, v: &mut [f64], offset: usize, stride: usize) {
        let dot: f64 = self
            .u
            .iter()
            .enumerate()
            .map(|(i, u)| u * v[offset + i * stride])
            .sum();
        if dot == 0.0 {
            return;
        }
        let scale = dot / self.b;
        for (i, u) in self.u.iter().enumerate() {
            v[offset + i * stride] += scale * u;
        }
    }
}

/// The factorization `A P = Q R` of an `m x n` matrix, truncated at its
/// numerical rank: `Q` is the product of `rank` reflectors and `R` is upper
/// trapezoidal in its first `rank` rows.
#[derive(Debug, Clone)]
pub(crate) struct Qr {
    r: Vec<f64>,
    cols: usize,
    reflectors: Vec<Reflector>,
    perm: Vec<usize>,
}

impl Qr {
    /// Factors `a`, choosing at each step the remaining column of largest norm
    /// and stopping once that norm falls to the rank tolerance.
    pub(crate) fn factor(a: MatRef<'_>) -> Self {
        let (m, n) = (a.rows, a.cols);
        let mut r: Vec<f64> = (0..m)
            .flat_map(|i| (0..n).map(move |j| a.at(i, j)))
            .collect();
        let mut perm: Vec<usize> = (0..n).collect();
        let mut reflectors = Vec::with_capacity(m.min(n));
        let mut tolerance = 0.0;
        for k in 0..m.min(n) {
            let (pivot, largest) = (k..n)
                .map(|j| (j, norm((k..m).map(|i| r[i * n + j]))))
                .fold(
                    (k, -1.0),
                    |best, next| if next.1 > best.1 { next } else { best },
                );
            if k == 0 {
                tolerance = m.max(n) as f64 * f64::EPSILON * largest;
            }
            if largest <= tolerance {
                break;
            }
            if pivot != k {
                for i in 0..m {
                    r.swap(i * n + k, i * n + pivot);
                }
                perm.swap(k, pivot);
            }
            let head = r[k * n + k];
            let s = if head > 0.0 { -largest } else { largest };
            let mut u: Vec<f64> = (k..m).map(|i| r[i * n + k]).collect();
            u[0] = head - s;
            let reflector = Reflector { b: s * u[0], u };
            r[k * n + k] = s;
            for i in k + 1..m {
                r[i * n + k] = 0.0;
            }
            for j in k + 1..n {
                reflector.apply(&mut r, k * n + j, n);
            }
            reflectors.push(reflector);
        }
        Self {
            r,
            cols: n,
            reflectors,
            perm,
        }
    }

    /// The numerical rank.
    pub(crate) fn rank(&self) -> usize {
        self.reflectors.len()
    }

    /// The column permutation: column `k` of `A P` is column `perm()[k]` of `A`.
    pub(crate) fn perm(&self) -> &[usize] {
        &self.perm
    }

    /// Overwrites `v`, of length `m`, with `Q' v`.
    pub(crate) fn apply_qt(&self, v: &mut [f64]) {
        for (k, reflector) in self.reflectors.iter().enumerate() {
            reflector.apply(v, k, 1);
        }
    }

    /// Overwrites `v`, of length `m`, with `Q v`.
    pub(crate) fn apply_q(&self, v: &mut [f64]) {
        for (k, reflector) in self.reflectors.iter().enumerate().rev() {
            reflector.apply(v, k, 1);
        }
    }

    /// Solves `R_11 z = y` in place on `y[..rank]`, `R_11` the leading
    /// nonsingular triangle.
    pub(crate) fn solve_r(&self, y: &mut [f64]) {
        let n = self.cols;
        for i in (0..self.rank()).rev() {
            let tail: f64 = (i + 1..self.rank()).map(|j| self.r[i * n + j] * y[j]).sum();
            y[i] = (y[i] - tail) / self.r[i * n + i];
        }
    }

    /// Solves `R_11' z = y` in place on `y[..rank]`.
    pub(crate) fn solve_rt(&self, y: &mut [f64]) {
        let n = self.cols;
        for i in 0..self.rank() {
            let head: f64 = (0..i).map(|j| self.r[j * n + i] * y[j]).sum();
            y[i] = (y[i] - head) / self.r[i * n + i];
        }
    }

    /// The basic least-squares solution of `A x = b`, with the coordinates
    /// past the rank set to zero, and its residual norm.
    pub(crate) fn solve(&self, b: &[f64]) -> (Vec<f64>, f64) {
        let mut y = b.to_vec();
        self.apply_qt(&mut y);
        let residual = norm(y[self.rank()..].iter().copied());
        self.solve_r(&mut y);
        let mut x = vec![0.0; self.cols];
        for k in 0..self.rank() {
            x[self.perm[k]] = y[k];
        }
        (x, residual)
    }
}
