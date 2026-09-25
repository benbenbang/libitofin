//! Sequential least-squares quadratic programming (SLSQP).
//!
//! Each iteration solves a quadratic model of the Lagrangian subject to the
//! linearized constraints, in the least-squares form of Kraft (1988), and
//! updates the Hessian approximation by Powell's damped BFGS formula.
//!
//! - Kraft, D. (1988), "A software package for sequential quadratic
//!   programming", DFVLR-FB 88-28, DLR German Aerospace Center: the
//!   least-squares subproblem and its relaxation.
//! - Powell, M. J. D. (1978), "A fast algorithm for nonlinearly constrained
//!   optimization calculations", Lecture Notes in Mathematics 630.
//! - Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
//!   Springer, Sections 18.3 and 18.4.
#![cfg_attr(not(test), allow(dead_code))]

mod hessian;
mod subproblem;
