//! Concrete 1-D solvers ported from `ql/math/solvers1d/`.
//!
//! Each solver implements [`Solver1D`](crate::math::solver1d::Solver1D) and so
//! shares the bracketing driver in [`solver1d`](crate::math::solver1d).

pub mod bisection;
pub mod brent;

#[cfg(test)]
pub(crate) mod testkit;
