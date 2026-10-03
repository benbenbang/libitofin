use crate::errors::QlResult;
use crate::math::distributions::normal::CumulativeNormalDistribution;
use crate::require;
use crate::types::Real;

#[derive(Clone, Copy)]
pub(super) struct Coefficients {
    pub m1: Real,
    pub m2: Real,
    pub m3: Real,
    pub v1: Real,
    pub v2: Real,
    pub z1: Real,
    pub x1: Real,
}

impl Coefficients {
    pub fn new(b1: Real, b2: Real, b3: Real, la: Real) -> QlResult<Self> {
        let n_cdf = CumulativeNormalDistribution::standard().value(la);
        let n = (-la * la / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
        let m1 = b1 + (b2 + b3 * n_cdf) * (1.0 + la * la) + b3 * la * n;
        let m2 = b1 * b1
            + b2 * b2 * (la.powi(4) + 6.0 * la * la + 3.0)
            + (b3 * b3 + 2.0 * b2 * b3)
                * (la.powi(4) * n_cdf
                    + la.powi(3) * n
                    + 6.0 * la * la * n_cdf
                    + 5.0 * la * n
                    + 3.0 * n_cdf)
            + 2.0 * b1 * b2 * (1.0 + la * la)
            + 2.0 * b3 * b1 * (la * la * n_cdf + la * n + n_cdf);
        let m3 = b1.powi(3)
            + (3.0 * b3 * b3 * b1 + 6.0 * b1 * b2 * b3)
                * (la.powi(3) * n
                    + 5.0 * la * n
                    + 3.0 * n_cdf
                    + la.powi(4) * n_cdf
                    + 6.0 * la * la * n_cdf)
            + b2.powi(3) * (15.0 + la.powi(6) + 15.0 * la.powi(4) + 45.0 * la * la)
            + (b3.powi(3) + 3.0 * b2 * b2 * b3 + 3.0 * b3 * b3 * b2)
                * (la.powi(5) * n
                    + 14.0 * la.powi(3) * n
                    + 33.0 * la * n
                    + 15.0 * n_cdf
                    + 15.0 * la.powi(4) * n_cdf
                    + 45.0 * la * la * n_cdf
                    + la.powi(6) * n_cdf)
            + 3.0 * b1 * b1 * b2 * (1.0 + la * la)
            + 3.0 * b1 * b1 * b3 * (la * n + n_cdf + la * la * n_cdf)
            + 3.0 * b1 * b2 * b2 * (3.0 + la.powi(4) + 6.0 * la * la);
        let v1 = -2.0 * b2 * la - 2.0 * b3 * (n + la * n_cdf);
        let v2 = -4.0 * b2 * b2 * (3.0 * la + la.powi(3))
            - (4.0 * b3 * b3 + 8.0 * b2 * b3)
                * (la * la * n + 2.0 * n + la.powi(3) * n_cdf + 3.0 * la * n_cdf)
            - 4.0 * b1 * b2 * la
            - 4.0 * b3 * b1 * (n + la * n_cdf);
        let z1 = b1 + b2 * (3.0 + la * la) + b3 * (la * n + 3.0 * n_cdf + la * la * n_cdf);
        let x1 = -6.0 * b2 * la - 2.0 * b3 * (4.0 * n + 3.0 * la * n_cdf);

        require!(
            [m1, m2, m3, v1, v2, z1, x1].iter().all(|x| x.is_finite()),
            "GJR-GARCH analytic coefficients are not finite"
        );
        for (a, b) in [
            (1.0, m1),
            (1.0, m2),
            (1.0, m3),
            (m1, m2),
            (m1, m3),
            (m2, m3),
        ] {
            let distinct = (a - b).abs() > 64.0 * Real::EPSILON * a.abs().max(b.abs()).max(1.0);
            require!(
                distinct,
                "GJR-GARCH analytic moment denominators are singular or ill-conditioned"
            );
        }
        Ok(Self {
            m1,
            m2,
            m3,
            v1,
            v2,
            z1,
            x1,
        })
    }
}
