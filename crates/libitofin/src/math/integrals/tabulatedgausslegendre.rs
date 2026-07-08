//! Tabulated Gauss-Legendre quadrature.
//!
//! Port of `TabulatedGaussLegendre` from `ql/math/integrals/gaussianquadratures`:
//! fixed-order Gauss-Legendre integration of a function over `[-1, 1]` using
//! tabulated weights and abscissae (Abramowitz and Stegun) for orders 6, 7, 12
//! and 20. The nodes are stored as the non-negative half and applied
//! symmetrically; an odd order additionally carries the central node at 0.

use crate::errors::QlResult;
use crate::fail;
use crate::types::Real;

// Abscissae (non-negative half) and weights, from Abramowitz and Stegun.
const X6: [Real; 3] = [0.238619186083197, 0.661209386466265, 0.932469514203152];
const W6: [Real; 3] = [0.467913934572691, 0.360761573048139, 0.171324492379170];

const X7: [Real; 4] = [
    0.000000000000000,
    0.405845151377397,
    0.741531185599394,
    0.949107912342759,
];
const W7: [Real; 4] = [
    0.417959183673469,
    0.381830050505119,
    0.279705391489277,
    0.129484966168870,
];

const X12: [Real; 6] = [
    0.125233408511469,
    0.367831498998180,
    0.587317954286617,
    0.769902674194305,
    0.904117256370475,
    0.981560634246719,
];
const W12: [Real; 6] = [
    0.249147045813403,
    0.233492536538355,
    0.203167426723066,
    0.160078328543346,
    0.106939325995318,
    0.047175336386512,
];

const X20: [Real; 10] = [
    0.076526521133497,
    0.227785851141645,
    0.373706088715420,
    0.510867001950827,
    0.636053680726515,
    0.746331906460151,
    0.839116971822219,
    0.912234428251326,
    0.963971927277914,
    0.993128599185095,
];
const W20: [Real; 10] = [
    0.152753387130726,
    0.149172986472604,
    0.142096109318382,
    0.131688638449177,
    0.118194531961518,
    0.101930119817240,
    0.083276741576704,
    0.062672048334109,
    0.040601429800387,
    0.017614007139152,
];

/// Fixed-order Gauss-Legendre quadrature on `[-1, 1]`.
#[derive(Clone, Copy, Debug)]
pub struct TabulatedGaussLegendre {
    order: usize,
    weights: &'static [Real],
    abscissae: &'static [Real],
}

impl TabulatedGaussLegendre {
    /// A Gauss-Legendre rule of the given `order`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `order` is one of the tabulated values 6, 7, 12,
    /// or 20.
    pub fn new(order: usize) -> QlResult<Self> {
        let (weights, abscissae): (&'static [Real], &'static [Real]) = match order {
            6 => (&W6, &X6),
            7 => (&W7, &X7),
            12 => (&W12, &X12),
            20 => (&W20, &X20),
            _ => fail!("Gauss-Legendre order {order} not supported (use 6, 7, 12, or 20)"),
        };
        Ok(TabulatedGaussLegendre {
            order,
            weights,
            abscissae,
        })
    }

    /// The order of the rule.
    pub fn order(&self) -> usize {
        self.order
    }

    /// Integrates `f` over `[-1, 1]`.
    ///
    /// The abscissae are stored as the non-negative half, so each is applied at
    /// `+x` and `-x`; an odd order treats the leading node (at 0) as the central
    /// point, counted once.
    pub fn integrate<F: Fn(Real) -> Real>(&self, f: F) -> Real {
        let mut value = 0.0;
        let mut start = 0;
        if self.order & 1 == 1 {
            // odd order: the first abscissa is the central node at 0
            value = self.weights[0] * f(self.abscissae[0]);
            start = 1;
        }
        for i in start..self.abscissae.len() {
            value += self.weights[i] * (f(self.abscissae[i]) + f(-self.abscissae[i]));
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDERS: [usize; 4] = [6, 7, 12, 20];

    fn assert_close(got: Real, expected: Real) {
        assert!(
            (got - expected).abs() < 1e-12,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn integrates_low_degree_polynomials_exactly() {
        // An order-n Gauss-Legendre rule is exact for polynomials up to degree
        // 2n-1, so every supported order integrates these exactly.
        for order in ORDERS {
            let gl = TabulatedGaussLegendre::new(order).unwrap();
            assert_close(gl.integrate(|_| 1.0), 2.0); // ∫ 1 = 2
            assert_close(gl.integrate(|x| x), 0.0); // ∫ x = 0 (odd)
            assert_close(gl.integrate(|x| x * x), 2.0 / 3.0); // ∫ x² = 2/3
            assert_close(gl.integrate(|x| x.powi(3)), 0.0); // ∫ x³ = 0 (odd)
            assert_close(gl.integrate(|x| x.powi(4)), 2.0 / 5.0); // ∫ x⁴ = 2/5
        }
    }

    #[test]
    fn integrates_exp_accurately_at_high_order() {
        // ∫_{-1}^{1} e^x dx = e - 1/e; order 20 nails it.
        let gl = TabulatedGaussLegendre::new(20).unwrap();
        let exact = std::f64::consts::E - 1.0 / std::f64::consts::E;
        assert_close(gl.integrate(|x| x.exp()), exact);
    }

    #[test]
    fn weights_sum_to_interval_length() {
        for order in ORDERS {
            let gl = TabulatedGaussLegendre::new(order).unwrap();
            assert_eq!(gl.order(), order);
            assert_close(gl.integrate(|_| 1.0), 2.0);
        }
    }

    #[test]
    fn rejects_unsupported_order() {
        assert!(TabulatedGaussLegendre::new(0).is_err());
        assert!(TabulatedGaussLegendre::new(5).is_err());
        assert!(TabulatedGaussLegendre::new(21).is_err());
    }
}
