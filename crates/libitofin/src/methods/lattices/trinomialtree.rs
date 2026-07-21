//! Recombining trinomial tree.
//!
//! Port of `ql/methods/lattices/trinomialtree.{hpp,cpp}`, the local variant.
//! It approximates a 1-D [`StochasticProcess1D`] on a [`TimeGrid`]; the
//! diffusion term must be independent of the state (`trinomialtree.hpp:36`).
//!
//! ## Local divergences from upstream QuantLib
//! This repo's C++ is a modified variant with a **dx floor** and **dual
//! probability regimes** absent from upstream. Both are ported faithfully:
//! - **dx floor** (`trinomialtree.cpp:57-92`): on a grid step much shorter than
//!   the longest (`dt < 0.01 * dtMax`) the spacing is widened to
//!   `max(v*sqrt(3), sqrt(3*max_i variance_i))`, preventing node explosion on a
//!   pathological tiny mandatory gap. On a uniform grid `dt == dtMax`, so the
//!   floor never fires and `dx = v*sqrt(3)`.
//! - **dual regimes** (`trinomialtree.cpp:112-144`): when the floor widened
//!   `dx`, general moment-matching weights are used and *signed* weights are
//!   accepted (documented, first two moments still matched); otherwise the
//!   classical Hull-White / Clewlow weights apply, kept in the exact upstream
//!   algebraic form so cached pricing values (bermudanswaption / callablebonds,
//!   #465) stay bit-for-bit identical.
//!
//! Two floating-point orders are reproduced deliberately: `v*sqrt(3)` for the
//! natural spacing (`cpp:78-79`) versus `sqrt(3*var)` for the floor (`cpp:67`).
//!
//! The negative-probability post-condition (`QL_ENSURE`, `cpp:156-165`) fires
//! only in the unfloored, un-bumped regime; there it maps to `Err`.

use crate::ensure;
use crate::errors::QlResult;
use crate::math::timegrid::TimeGrid;
use crate::methods::lattices::tree::Tree;
use crate::require;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Integer, Real, Size, Time};

/// Floor activation threshold: the dx floor is applied only on grid steps
/// shorter than `K_FLOOR_THRESHOLD * dtMax` (`trinomialtree.cpp:32`).
const K_FLOOR_THRESHOLD: Real = 0.01;

/// Recombining trinomial tree approximating a 1-D stochastic process.
pub struct TrinomialTree {
    branchings: Vec<Branching>,
    x0: Real,
    dx: Vec<Real>,
    time_grid: TimeGrid,
    columns: Size,
}

impl TrinomialTree {
    /// Builds the tree for `process` over `time_grid`. When `is_positive` is
    /// set, the middle branch is bumped up so the underlying stays positive
    /// (`trinomialtree.cpp:102-107`), as CIR-family models require.
    ///
    /// # Errors
    /// Returns `Err` if the grid has no steps, if a process query fails, or if
    /// a transition probability is negative in the unfloored, un-bumped regime
    /// (`QL_ENSURE`, `cpp:156`).
    pub fn new(
        process: Shared<dyn StochasticProcess1D>,
        time_grid: TimeGrid,
        is_positive: bool,
    ) -> QlResult<Self> {
        let x0 = process.x0()?;
        let columns = time_grid.size();
        let n_time_steps = columns - 1;
        require!(n_time_steps > 0, "null time steps for trinomial tree");

        let mut dts = Vec::with_capacity(n_time_steps);
        let mut v2_cache = Vec::with_capacity(n_time_steps);
        for i in 0..n_time_steps {
            let dt_i = time_grid.dt(i);
            dts.push(dt_i);
            v2_cache.push(process.variance(time_grid[i], 0.0, dt_i)?);
        }
        let (dx, floored) = dx_schedule(&dts, &v2_cache);

        let mut branchings: Vec<Branching> = Vec::with_capacity(n_time_steps);
        let mut j_min: Integer = 0;
        let mut j_max: Integer = 0;
        for i in 0..n_time_steps {
            let t = time_grid[i];
            let dt = dts[i];
            let v2 = v2_cache[i];
            let v = v2.sqrt();
            let dx_next = dx[i + 1];
            let dx_is_floored = floored[i];
            let dx2 = dx_next * dx_next;

            let mut branching = Branching::new();
            for j in j_min..=j_max {
                let x = x0 + j as Real * dx[i];
                let m = process.expectation(t, x, dt)?;
                let mut temp = ((m - x0) / dx[i + 1] + 0.5).floor() as Integer;

                let mut temp_bumped = false;
                if is_positive {
                    while x0 + (temp - 1) as Real * dx[i + 1] <= 0.0 {
                        temp += 1;
                        temp_bumped = true;
                    }
                }

                let e = m - (x0 + temp as Real * dx[i + 1]);
                let e2 = e * e;

                let (p1, p2, p3) = if dx_is_floored {
                    (
                        (v2 + e2 - e * dx_next) / (2.0 * dx2),
                        1.0 - (v2 + e2) / dx2,
                        (v2 + e2 + e * dx_next) / (2.0 * dx2),
                    )
                } else {
                    let e3 = e * 3.0_f64.sqrt();
                    (
                        (1.0 + e2 / v2 - e3 / v) / 6.0,
                        (2.0 - e2 / v2) / 3.0,
                        (1.0 + e2 / v2 + e3 / v) / 6.0,
                    )
                };

                if !dx_is_floored && !temp_bumped {
                    ensure!(
                        p1 >= 0.0 && p2 >= 0.0 && p3 >= 0.0,
                        "negative probability in trinomial tree (unfloored regime) \
                         at step {i}, node {j}: p1={p1}, p2={p2}, p3={p3} \
                         (v={v}, dx={dx_next}, e={e})"
                    );
                }

                branching.add(temp, p1, p2, p3);
            }
            j_min = branching.j_min();
            j_max = branching.j_max();
            branchings.push(branching);
        }

        Ok(TrinomialTree {
            branchings,
            x0,
            dx,
            time_grid,
            columns,
        })
    }

    /// The state spacing `dx_[i]` at slice `i` (`trinomialtree.hpp:48`).
    pub fn dx(&self, i: Size) -> Real {
        self.dx[i]
    }

    /// The time grid the tree was built on (`trinomialtree.hpp:49`).
    pub fn time_grid(&self) -> &TimeGrid {
        &self.time_grid
    }
}

impl Tree for TrinomialTree {
    const BRANCHES: Size = 3;

    fn columns(&self) -> Size {
        self.columns
    }

    fn size(&self, i: Size) -> Size {
        if i == 0 {
            1
        } else {
            self.branchings[i - 1].size()
        }
    }

    fn underlying(&self, i: Size, index: Size) -> Real {
        if i == 0 {
            self.x0
        } else {
            self.x0 + (self.branchings[i - 1].j_min() as Real + index as Real) * self.dx(i)
        }
    }

    fn descendant(&self, i: Size, index: Size, branch: Size) -> Size {
        self.branchings[i].descendant(index, branch)
    }

    fn probability(&self, i: Size, index: Size, branch: Size) -> Real {
        self.branchings[i].probability(index, branch)
    }
}

/// Computes the per-slice spacing and the effective-floor flag in one pass.
///
/// Returns `(dx, floored)` where `dx` has `dts.len() + 1` entries (`dx[0] = 0`,
/// mirroring the C++ `dx_(1, 0.0)` seed) and `floored[i]` records whether the
/// floor *widened* `dx[i + 1]` beyond its natural `v*sqrt(3)` value, not merely
/// whether the gate condition fired (`trinomialtree.cpp:85-92`). Factoring this
/// out keeps the two `v*sqrt(3)` computations a single expression, so the flag
/// cannot flip on a last-ULP mismatch, and lets the floored branch (unreachable
/// through the regular-only [`TimeGrid`] ctor) be exercised directly.
fn dx_schedule(dts: &[Time], v2s: &[Real]) -> (Vec<Real>, Vec<bool>) {
    let dt_max = dts.iter().copied().fold(0.0_f64, Real::max);
    let dx_floor_var = v2s.iter().copied().fold(0.0_f64, Real::max);
    let dx_floor = (3.0 * dx_floor_var).sqrt();

    let mut dx = Vec::with_capacity(dts.len() + 1);
    let mut floored = Vec::with_capacity(dts.len());
    dx.push(0.0);
    for (i, &dt) in dts.iter().enumerate() {
        let dx_natural = v2s[i].sqrt() * 3.0_f64.sqrt();
        let dx_next = if dt < K_FLOOR_THRESHOLD * dt_max {
            dx_natural.max(dx_floor)
        } else {
            dx_natural
        };
        floored.push(dx_next > dx_natural);
        dx.push(dx_next);
    }
    (dx, floored)
}

/// Branching scheme for one trinomial slice.
///
/// Port of the inner `Branching` (`trinomialtree.hpp:66-143`). Each node's
/// middle branch links to `k_[index]`, the next-slice node closest to the
/// node's expectation; the three descendants centre on it. `j_min`/`j_max`
/// track the widening index range as nodes are added.
struct Branching {
    k: Vec<Integer>,
    probs: [Vec<Real>; 3],
    k_min: Integer,
    j_min: Integer,
    k_max: Integer,
    j_max: Integer,
}

impl Branching {
    fn new() -> Self {
        Branching {
            k: Vec::new(),
            probs: [Vec::new(), Vec::new(), Vec::new()],
            k_min: Integer::MAX,
            j_min: Integer::MAX,
            k_max: Integer::MIN,
            j_max: Integer::MIN,
        }
    }

    fn descendant(&self, index: Size, branch: Size) -> Size {
        (self.k[index] - self.j_min - 1 + branch as Integer) as Size
    }

    fn probability(&self, index: Size, branch: Size) -> Real {
        self.probs[branch][index]
    }

    fn size(&self) -> Size {
        (self.j_max - self.j_min + 1) as Size
    }

    fn j_min(&self) -> Integer {
        self.j_min
    }

    fn j_max(&self) -> Integer {
        self.j_max
    }

    fn add(&mut self, k: Integer, p1: Real, p2: Real, p3: Real) {
        self.k.push(k);
        self.probs[0].push(p1);
        self.probs[1].push(p2);
        self.probs[2].push(p3);
        self.k_min = self.k_min.min(k);
        self.j_min = self.k_min - 1;
        self.k_max = self.k_max.max(k);
        self.j_max = self.k_max + 1;
    }
}
