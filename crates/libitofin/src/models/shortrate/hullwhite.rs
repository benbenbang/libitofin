//! The Hull-White one-factor short-rate model.
//!
//! Port of `ql/models/shortrate/onefactormodels/hullwhite.{hpp,cpp}`: the
//! extended-Vasicek short rate `dr_t = (theta(t) - a r_t) dt + sigma dW_t`,
//! whose deterministic drift `theta(t)` is fitted so the model reprices the
//! input [`YieldTermStructure`](crate::termstructures::yieldtermstructure::YieldTermStructure)
//! exactly. This slice ports the closed-form curve-fit and the static futures
//! convexity bias; the model itself and its oracles land with the following
//! commits.
//!
//! ## Deferred (omitted, not stubbed)
//!
//! - `discountBondOption` (`hullwhite.hpp:55`/`:60`, `hullwhite.cpp:90`/`:108`),
//!   the Jamshidian bond option, needs `blackFormula` and has no oracle in this
//!   batch; porting it now ships unpinned code (#262 rule).
//! - `HullWhite::Dynamics` (`hullwhite.hpp:107`), `tree` (`hullwhite.cpp:43`),
//!   `FixedReversion` (`hullwhite.hpp:80`) and the `JamshidianSwaptionEngine`
//!   swaption oracles (`testCachedHullWhite*`) are the simulation/lattice and
//!   calibration paths, deferred with the short-rate dynamics per #377.

use crate::errors::QlResult;
use crate::require;
use crate::types::{Rate, Real, Time};

/// `HullWhite::convexityBias(Real futuresPrice, Time t, Time T, Real sigma,
/// Real a)` (`hullwhite.cpp:134`): the futures convexity bias (the difference
/// between the futures-implied rate and the forward rate), computed as in G.
/// Kirikos, D. Novak, "Convexity Conundrums", Risk Magazine, March 1997.
///
/// `t` and `T` are year fractions in the deposit day counter, and `futures_price`
/// is the futures' market price. C++ maps this static member to a namespaced free
/// function here (no instance is needed).
///
/// The small-mean-reversion guard is plain `QL_EPSILON` (`hullwhite.cpp:150`),
/// distinct from the `sqrt(QL_EPSILON)` guard on `Vasicek::B` and on the
/// (deferred) fitting law: `temp(x) = a < QL_EPSILON ? x : (1 - e^{-ax})/a`.
///
/// # Errors
///
/// Mirrors the five `QL_REQUIRE`s (`hullwhite.cpp:139-148`): fails on a negative
/// futures price, a negative `t`, `T < t`, a negative `sigma`, or a negative `a`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn convexity_bias(
    futures_price: Real,
    t: Time,
    maturity: Time,
    sigma: Real,
    a: Real,
) -> QlResult<Rate> {
    require!(
        futures_price >= 0.0,
        "negative futures price ({futures_price}) not allowed"
    );
    require!(t >= 0.0, "negative t ({t}) not allowed");
    require!(
        maturity >= t,
        "T ({maturity}) must not be less than t ({t})"
    );
    require!(sigma >= 0.0, "negative sigma ({sigma}) not allowed");
    require!(a >= 0.0, "negative a ({a}) not allowed");

    let temp = |x: Real| {
        if a < Real::EPSILON {
            x
        } else {
            (1.0 - (-a * x).exp()) / a
        }
    };

    let delta_t = maturity - t;
    let temp_delta_t = temp(delta_t);
    let half_sigma_square = sigma * sigma / 2.0;
    let lambda = temp(2.0 * t) * temp_delta_t;
    let temp_t = temp(t);
    let phi = temp_t * temp_t;
    let z = half_sigma_square * (lambda + phi);
    let future_rate = (100.0 - futures_price) / 100.0;
    if delta_t < Real::EPSILON {
        Ok(z)
    } else {
        Ok((1.0 - (-z * temp_delta_t).exp()) * (future_rate + 1.0 / delta_t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn futures_convexity_bias_reproduces_the_kirikos_novak_table() {
        // testFuturesConvexityBias (shortratemodels.cpp:407-438). G. Kirikos, D.
        // Novak, "Convexity Conundrums", Risk Magazine, March 1997. The five rows
        // exercise all three branches of the body: general a (0.03), the
        // small-a threshold (1e-4, below QL_EPSILON only where 2t makes it bite),
        // a == 0, and deltaT -> 0 (T = 5.001 and T = t = 5.0).
        let future_quote = 94.0;
        let sigma = 0.015;
        let t = 5.0;
        let tolerance = 1e-7;
        let future_implied_rate = (100.0 - future_quote) / 100.0;

        for (maturity, a, expected_forward) in [
            (5.25, 0.03, 0.0573037),
            (5.25, 1e-4, 0.0568627),
            (5.25, 0.0, 0.0568611),
            (5.001, 0.03, 0.0575736),
            (5.0, 0.03, 0.0575747),
        ] {
            let bias = convexity_bias(future_quote, t, maturity, sigma, a).unwrap();
            let calculated_forward = future_implied_rate - bias;
            assert!(
                (calculated_forward - expected_forward).abs() < tolerance,
                "T={maturity}, a={a}: got {calculated_forward}, expected {expected_forward}"
            );
        }
    }

    #[test]
    fn convexity_bias_rejects_out_of_range_inputs_with_the_cpp_messages() {
        assert_eq!(
            convexity_bias(-1.0, 5.0, 5.25, 0.015, 0.03)
                .unwrap_err()
                .message(),
            "negative futures price (-1) not allowed"
        );
        assert_eq!(
            convexity_bias(94.0, -5.0, 5.25, 0.015, 0.03)
                .unwrap_err()
                .message(),
            "negative t (-5) not allowed"
        );
        assert_eq!(
            convexity_bias(94.0, 5.0, 4.0, 0.015, 0.03)
                .unwrap_err()
                .message(),
            "T (4) must not be less than t (5)"
        );
        assert_eq!(
            convexity_bias(94.0, 5.0, 5.25, -0.015, 0.03)
                .unwrap_err()
                .message(),
            "negative sigma (-0.015) not allowed"
        );
        assert_eq!(
            convexity_bias(94.0, 5.0, 5.25, 0.015, -0.03)
                .unwrap_err()
                .message(),
            "negative a (-0.03) not allowed"
        );
    }
}
