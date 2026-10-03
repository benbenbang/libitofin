use super::coefficients::Coefficients;
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

#[derive(Clone, Copy)]
pub(super) struct Moments {
    pub mean: Real,
    pub variance: Real,
    pub skewness: Real,
    pub kurtosis: Real,
}

impl Moments {
    pub fn new(c: Coefficients, b0: Real, h1: Real, days: usize, r: Real) -> QlResult<Self> {
        let Coefficients {
            m1,
            m2,
            m3,
            v1,
            v2,
            z1,
            x1,
        } = c;
        let mut m1ai = vec![1.0; days];
        let mut m2ai = vec![1.0; days];
        let mut m3ai = vec![1.0; days];
        for i in 1..days {
            m1ai[i] = m1ai[i - 1] * m1;
            m2ai[i] = m2ai[i - 1] * m2;
            m3ai[i] = m3ai[i - 1] * m3;
        }
        require!(
            m1ai.iter().chain(&m2ai).chain(&m3ai).all(|x| x.is_finite()),
            "GJR-GARCH moment powers overflow"
        );
        let mut s_eh = 0.0;
        let mut s_eh2 = 0.0;
        let mut s_ehh = 0.0;
        let mut s_eh1_2eh = 0.0;
        let mut s_ehhh = 0.0;
        let mut s_eh2h = 0.0;
        let mut s_ehh2 = 0.0;
        let mut s_eh3 = 0.0;
        let mut s_eh1_2eh2 = 0.0;
        let mut s_eh3_2eh = 0.0;
        let mut s_eh1_2ehh = 0.0;
        let mut s_ehh1_2eh = 0.0;
        let mut s_ehe2h = 0.0;
        let mut s_eh1_2eh1_2eh = 0.0;
        let mut s_eh3_2e3h = 0.0;
        for i in 0..days {
            let m1i = m1ai[i];
            let m2i = m2ai[i];
            let m3i = m3ai[i];

            let m1im2i = m1i - m2i;
            let m1im3i = m1i - m3i;
            let m2im3i = m2i - m3i;
            let eh = b0 * (1.0 - m1i) / (1.0 - m1) + m1i * h1;
            let eh2 =
                b0 * b0 * ((1.0 + m1) * (1.0 - m2i) / (1.0 - m2) - 2.0 * m1 * m1im2i / (m1 - m2))
                    / (1.0 - m1)
                    + 2.0 * b0 * m1 * m1im2i * h1 / (m1 - m2)
                    + m2i * h1 * h1;
            let eh3 = b0.powf(3.0)
                * ((1.0 - m3i) / (1.0 - m3)
                    + 3.0 * m2 * ((1.0 - m3i) / (1.0 - m3) - m2im3i / (m2 - m3)) / (1.0 - m2)
                    + 3.0 * m1 * ((1.0 - m3i) / (1.0 - m3) - m1im3i / (m1 - m3)) / (1.0 - m1)
                    + 6.0
                        * m1
                        * m2
                        * (((1.0 - m3i) / (1.0 - m3) - m2im3i / (m2 - m3)) / (1.0 - m2)
                            + (m2im3i / (m2 - m3) - m1im3i / (m1 - m3)) / (m1 - m2))
                        / (1.0 - m1))
                + 3.0
                    * b0
                    * b0
                    * m1
                    * h1
                    * (m1im3i / (m1 - m3)
                        + 2.0 * m2 * (m1im3i / (m1 - m3) - m2im3i / (m2 - m3)) / (m1 - m2))
                + 3.0 * b0 * m2 * h1 * h1 * m2im3i / (m2 - m3)
                + m3i * h1 * h1 * h1;
            require!(
                eh.is_finite() && eh > 0.0 && eh2.is_finite() && eh3.is_finite(),
                "GJR-GARCH daily moments are invalid or nonfinite"
            );
            let eh3_2 = 0.375 * eh.powf(-0.5) * eh2 + 0.625 * eh.powf(1.5);
            let eh5_2 = 1.875 * eh.powf(0.5) * eh2 - 0.875 * eh.powf(2.5);
            s_eh += eh;
            s_eh2 += eh2;
            s_eh3 += eh3;
            for j in 0..days - i - 1 {
                let ehh = b0 * eh * (1.0 - m1ai[j + 1]) / (1.0 - m1) + eh2 * m1ai[j + 1];
                let ehh2 = b0
                    * b0
                    * eh
                    * ((1.0 + m1) * (1.0 - m2ai[j + 1]) / (1.0 - m2)
                        - 2.0 * m1 * (m1ai[j + 1] - m2ai[j + 1]) / (m1 - m2))
                    / (1.0 - m1)
                    + 2.0 * b0 * m1 * eh2 * (m1ai[j + 1] - m2ai[j + 1]) / (m1 - m2)
                    + m2ai[j + 1] * eh3;
                let eh2h = b0 * eh2 * (1.0 - m1ai[j + 1]) / (1.0 - m1) + m1ai[j + 1] * eh3;
                let eh1_2eh = v1 * m1ai[j] * eh3_2;
                let eh1_2eh2 = 2.0 * b0 * v1 * (m1ai[j + 1] - m2ai[j + 1]) * eh3_2 / (m1 - m2)
                    + v2 * m2ai[j] * eh5_2;
                let ehij = b0 * (1.0 - m1ai[i + j + 1]) / (1.0 - m1) + m1ai[i + j + 1] * h1;
                require!(
                    ehij.is_finite() && ehij > 0.0,
                    "GJR-GARCH intermediate expected variance is not finite and positive"
                );
                let ehh3_2 = 0.375 * ehh2 / ehij.sqrt() + 0.75 * ehij.sqrt() * ehh
                    - 0.125 * ehij.powf(1.5) * eh;
                let eh3_2eh = v1 * m1ai[j] * eh5_2;
                let eh3_2e3h = x1 * m1ai[j] * eh5_2;
                let eh1_2eh3_2 = 0.375 * eh1_2eh2 / ehij.sqrt() + 0.75 * ehij.sqrt() * eh1_2eh;
                s_ehh += ehh;
                s_eh1_2eh += eh1_2eh;
                s_ehh2 += ehh2;
                s_eh2h += eh2h;
                s_eh1_2eh2 += eh1_2eh2;
                s_eh3_2eh += eh3_2eh;
                s_ehe2h += b0 * eh * (1.0 - m1ai[j + 1]) / (1.0 - m1) + z1 * m1ai[j] * eh2;
                s_eh3_2e3h += eh3_2e3h;
                for k in 0..days - i - j - 2 {
                    let ehhh = b0 * ehh * (1.0 - m1ai[k + 1]) / (1.0 - m1) + m1ai[k + 1] * ehh2;
                    let eh1_2ehh =
                        b0 * eh1_2eh * (1.0 - m1ai[k + 1]) / (1.0 - m1) + m1ai[k + 1] * eh1_2eh2;
                    s_ehhh += ehhh;
                    s_eh1_2ehh += eh1_2ehh;
                    s_ehh1_2eh += v1 * m1ai[k] * ehh3_2;
                    s_eh1_2eh1_2eh += v1 * m1ai[k] * eh1_2eh3_2;
                }
            }
        }

        let t = days as Real;
        let ex = t * r - 0.5 * s_eh;
        let sd1 = 2.0 * s_ehh + s_eh2;
        let sd2 = s_eh;
        let sd3 = s_eh1_2eh;
        let ex2 = t * t * r * r - t * r * s_eh + 0.25 * sd1 + sd2 - sd3;
        let st1 = 6.0 * s_ehhh + (3.0 * s_ehh2 + (3.0 * s_eh2h + s_eh3));
        let st2 = 3.0 * s_eh1_2eh;
        let st3 = 2.0 * s_ehh1_2eh + (2.0 * s_eh1_2ehh + (2.0 * s_eh3_2eh + s_eh1_2eh2));
        let st4 = s_ehe2h + (s_ehh + (s_eh2 + 2.0 * s_eh1_2eh1_2eh));
        let ex3 = (t * r).powi(3) - 1.5 * t * t * r * r * s_eh
            + 3.0 * t * r * (sd1 / 4.0 + sd2 - sd3)
            + (st2 - st1 / 8.0 + 3.0 * st3 / 4.0 - 3.0 * st4 / 2.0);
        let sq2 = 6.0 * s_ehe2h + (12.0 * s_eh1_2eh1_2eh + 3.0 * s_eh2);
        let sq4 = 2.0 * s_ehhh + 2.0 * s_ehh2;
        let sq5 =
            3.0 * s_ehh1_2eh + 3.0 * s_eh1_2ehh + 3.0 * s_eh3_2eh + 3.0 * s_eh1_2eh2 + s_eh3_2e3h;
        let ex4 = (t * r).powi(4) - 2.0 * (t * r).powi(3) * s_eh
            + 6.0 * t * t * r * r * (sd1 / 4.0 + sd2 - sd3)
            + t * r * (4.0 * st2 - st1 / 2.0 + 3.0 * st3 - 6.0 * st4)
            + (sq2 + 3.0 * sq4 / 2.0 - 2.0 * sq5);

        let sigma = ex2 - ex * ex;

        require!(
            sigma.is_finite() && sigma > 0.0,
            "GJR-GARCH analytic variance must be finite and positive"
        );
        let mut k3 = ex3 - 3.0 * sigma * ex - ex * ex * ex;

        let mut k4 = ex4 + 6.0 * ex * ex * ex2 - 3.0 * ex * ex * ex * ex - 4.0 * ex * ex3;
        k3 /= sigma.powf(1.5);
        k4 /= sigma.powf(2.0);

        require!(
            [ex, sigma, k3, k4].iter().all(|x| x.is_finite()),
            "GJR-GARCH analytic moments are not finite"
        );
        Ok(Self {
            mean: ex,
            variance: sigma,
            skewness: k3,
            kurtosis: k4,
        })
    }
}
