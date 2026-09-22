//! COS Heston cumulants adapted from QuantLib 1.43.
//! Copyright (C) 2017 Klaus Spanderen, (C) 2022 Ignacio Anguita.
//! Distributed under the QuantLib license; see `THIRD_PARTY_NOTICES.md`.

use super::analytichestonengine::HestonChf;
use crate::types::Real;

pub(super) fn c1(p: HestonChf, t: Real) -> Real {
    let (kappa, theta, v0) = (p.kappa(), p.theta(), p.v0());
    (-theta + (kappa * t).exp() * (theta - kappa * t * theta - v0) + v0)
        / (2.0 * (kappa * t).exp() * kappa)
}

pub(super) fn c2(p: HestonChf, t: Real) -> Real {
    let (kappa, theta, sigma, rho, v0) = (p.kappa(), p.theta(), p.sigma(), p.rho(), p.v0());
    let sigma2 = sigma * sigma;
    let kappa2 = kappa * kappa;
    let kappa3 = kappa2 * kappa;

    (sigma2 * (theta - 2.0 * v0)
        + (2.0 * kappa * t).exp()
            * (8.0 * kappa3 * t * theta - 8.0 * kappa2 * (theta + rho * sigma * t * theta - v0)
                + sigma2 * (-5.0 * theta + 2.0 * v0)
                + 2.0 * kappa * sigma * (8.0 * rho * theta + sigma * t * theta - 4.0 * rho * v0))
        + 4.0
            * (kappa * t).exp()
            * (sigma2 * theta - 2.0 * kappa2 * (-1.0 + rho * sigma * t) * (theta - v0)
                + kappa * sigma * (sigma * t * (theta - v0) + 2.0 * rho * (-2.0 * theta + v0))))
        / (8. * (2.0 * kappa * t).exp() * kappa3)
}

pub(super) fn c3(p: HestonChf, t: Real) -> Real {
    let (kappa, theta, sigma, rho, v0) = (p.kappa(), p.theta(), p.sigma(), p.rho(), p.v0());
    let sigma2 = sigma * sigma;
    let sigma3 = sigma2 * sigma;
    let kappa2 = kappa * kappa;
    let kappa3 = kappa2 * kappa;
    let kappa4 = kappa3 * kappa;
    let rho2 = rho * rho;

    -(sigma
        * (sigma3 * (theta - 3.0 * v0)
            + (3.0 * kappa * t).exp()
                * (2.0
                    * (-11.0 * sigma3 - 24.0 * kappa4 * rho * t
                        + 3.0 * kappa * sigma2 * (20.0 * rho + sigma * t)
                        - 6.0 * kappa2 * sigma * (5.0 + 3.0 * rho * (4.0 * rho + sigma * t))
                        + 12.0 * kappa3 * (sigma * t + 2.0 * rho * (2.0 + rho * sigma * t)))
                    * theta
                    - 6.0
                        * (2.0 * kappa * rho - sigma)
                        * (4.0 * kappa2 - 4.0 * kappa * rho * sigma + sigma2)
                        * v0)
            + 6.0
                * (kappa * t).exp()
                * sigma
                * (-2.0 * kappa2 * (-1.0 + rho * sigma * t) * (theta - 2.0 * v0)
                    + sigma2 * (theta - v0)
                    + kappa
                        * sigma
                        * (-4.0 * rho * theta + sigma * t * theta + 6.0 * rho * v0
                            - 2.0 * sigma * t * v0))
            + 3.0
                * (2.0 * kappa * t).exp()
                * (2.0 * kappa * sigma2 * (-16.0 * rho * theta + sigma * t * (3.0 * theta - v0))
                    + 8.0 * kappa4 * rho * t * (-2.0 + rho * sigma * t) * (theta - v0)
                    + sigma3 * (5.0 * theta + v0)
                    + 8.0
                        * kappa3
                        * (-(rho * (4.0 + sigma2 * t * t) * theta)
                            + 2.0 * sigma * t * (theta - v0)
                            + 2.0 * rho2 * sigma * t * (2.0 * theta - v0)
                            + rho * (2.0 + sigma2 * t * t) * v0)
                    + 2.0
                        * kappa2
                        * sigma
                        * ((8.0 + 24.0 * rho2 - 16.0 * rho * sigma * t + sigma2 * t * t) * theta
                            - (8.0 * rho2 - 8.0 * rho * sigma * t + sigma2 * t * t) * v0))))
        / (16. * (3.0 * kappa * t).exp() * kappa * kappa4)
}

pub(super) fn c4(p: HestonChf, t: Real) -> Real {
    let (kappa, theta, sigma, rho, v0) = (p.kappa(), p.theta(), p.sigma(), p.rho(), p.v0());
    let sigma2 = sigma * sigma;
    let sigma3 = sigma2 * sigma;
    let sigma4 = sigma2 * sigma2;
    let kappa2 = kappa * kappa;
    let kappa3 = kappa2 * kappa;
    let kappa4 = kappa2 * kappa2;
    let kappa5 = kappa2 * kappa3;
    let kappa6 = kappa3 * kappa3;
    let kappa7 = kappa4 * kappa3;
    let rho2 = rho * rho;
    let rho3 = rho2 * rho;
    let t2 = t * t;
    let t3 = t2 * t;

    (sigma2
        * (3.0 * sigma4 * (theta - 4.0 * v0)
            + 3.0
                * (4.0 * kappa * t).exp()
                * ((-93.0 * sigma4
                    + 64.0 * kappa5 * (t + 4.0 * rho2 * t)
                    + 4.0 * kappa * sigma3 * (176.0 * rho + 5.0 * sigma * t)
                    - 32.0 * kappa2 * sigma2 * (11.0 + 50.0 * rho2 + 5.0 * rho * sigma * t)
                    + 32.0
                        * kappa3
                        * sigma
                        * (3.0 * sigma * t
                            + 4.0 * rho * (10.0 + 8.0 * rho2 + 3.0 * rho * sigma * t))
                    - 32.0
                        * kappa4
                        * (5.0 + 4.0 * rho * (6.0 * rho + (3.0 + 2.0 * rho2) * sigma * t)))
                    * theta
                    + 4.0
                        * (4.0 * kappa2 - 4.0 * kappa * rho * sigma + sigma2)
                        * (4.0 * kappa2 * (1.0 + 4.0 * rho2) - 20.0 * kappa * rho * sigma
                            + 5.0 * sigma2)
                        * v0)
            + 24.0
                * (kappa * t).exp()
                * sigma2
                * (-2.0 * kappa2 * (-1.0 + rho * sigma * t) * (theta - 3.0 * v0)
                    + sigma2 * (theta - 2.0 * v0)
                    + kappa
                        * sigma
                        * (-4.0 * rho * theta + sigma * t * theta + 10.0 * rho * v0
                            - 3.0 * sigma * t * v0))
            + 12.0
                * (2.0 * kappa * t).exp()
                * (sigma4 * (7.0 * theta - 4.0 * v0)
                    + 8.0
                        * kappa4
                        * (1.0 + 2.0 * rho * sigma * t * (-2.0 + rho * sigma * t))
                        * (theta - 2.0 * v0)
                    + 2.0
                        * kappa
                        * sigma3
                        * (-24.0 * rho * theta + 5.0 * sigma * t * theta + 20.0 * rho * v0
                            - 6.0 * sigma * t * v0)
                    + 4.0
                        * kappa2
                        * sigma2
                        * ((6.0 + 20.0 * rho2 - 14.0 * rho * sigma * t + sigma2 * t2) * theta
                            - 2.0
                                * (3.0 + 12.0 * rho2 - 10.0 * rho * sigma * t + sigma2 * t2)
                                * v0)
                    + 8.0
                        * kappa3
                        * sigma
                        * ((3.0 * sigma * t
                            + 2.0 * rho * (-4.0 + sigma * t * (4.0 * rho - sigma * t)))
                            * theta
                            + 2.0
                                * (-3.0 * sigma * t
                                    + 2.0 * rho * (3.0 + sigma * t * (-3.0 * rho + sigma * t)))
                                * v0))
            - 8.0
                * (3.0 * kappa * t).exp()
                * (16.0 * kappa6 * rho2 * t2 * (-3.0 + rho * sigma * t) * (theta - v0)
                    - 3.0 * sigma4 * (7.0 * theta + 2.0 * v0)
                    + 2.0
                        * kappa3
                        * sigma
                        * ((192.0 * (rho + rho3) - 6.0 * (9.0 + 40.0 * rho2) * sigma * t
                            + 42.0 * rho * sigma2 * t2
                            - sigma3 * t3)
                            * theta
                            + (-48.0 * rho3 + 18.0 * (1.0 + 4.0 * rho2) * sigma * t
                                - 24.0 * rho * sigma2 * t2
                                + sigma3 * t3)
                                * v0)
                    + 12.0
                        * kappa4
                        * ((-4.0 - 24.0 * rho2 + 8.0 * rho * (4.0 + 3.0 * rho2) * sigma * t
                            - (3.0 + 14.0 * rho2) * sigma2 * t2
                            + rho * sigma3 * t3)
                            * theta
                            + (8.0 * rho2 - 8.0 * rho * (2.0 + rho2) * sigma * t
                                + (3.0 + 8.0 * rho2) * sigma2 * t2
                                - rho * sigma3 * t3)
                                * v0)
                    - 6.0
                        * kappa2
                        * sigma2
                        * ((15.0 + 80.0 * rho2 - 35.0 * rho * sigma * t + 2.0 * sigma2 * t2)
                            * theta
                            + (3.0 + sigma * t * (7.0 * rho - sigma * t)) * v0)
                    + 24.0
                        * kappa5
                        * t
                        * ((-2.0
                            + rho
                                * (4.0 * sigma * t
                                    + rho * (-8.0 + sigma * t * (4.0 * rho - sigma * t))))
                            * theta
                            + (2.0
                                + rho
                                    * (-4.0 * sigma * t
                                        + rho * (4.0 + sigma * t * (-2.0 * rho + sigma * t))))
                                * v0)
                    + 3.0
                        * kappa
                        * sigma3
                        * (sigma * t * (-9.0 * theta + v0) + 10.0 * rho * (6.0 * theta + v0)))))
        / (64. * (4.0 * kappa * t).exp() * kappa7)
}
