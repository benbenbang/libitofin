//! Fang-Oosterlee COS Heston engine, adapted from QuantLib 1.43.
//! Copyright (C) 2017 Klaus Spanderen, (C) 2022 Ignacio Anguita.
//! Distributed under the QuantLib license; see `THIRD_PARTY_NOTICES.md`.

use super::{analytichestonengine::HestonChf, hestoncumulants, hestonmarket::HestonMarket};
use crate::errors::QlResult;
use crate::instruments::{OneAssetOptionEngine, OneAssetOptionResults, OptionArguments};
use crate::models::{HestonModel, model::CalibratedModelHolder};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::require;
use crate::shared::SharedMut;
use crate::types::{Complex, Real, Size, Time};

/// European plain-vanilla option prices using a Fourier cosine expansion.
pub struct CosHestonEngine {
    base: OneAssetOptionEngine,
    model: SharedMut<HestonModel>,
    l: Real,
    n: Size,
}

impl CosHestonEngine {
    /// Builds an engine with a positive finite truncation width and a nonzero series size.
    pub fn new(model: SharedMut<HestonModel>, l: Real, n: Size) -> QlResult<Self> {
        require!(
            l.is_finite() && l > 0.0,
            "COS truncation width must be positive and finite"
        );
        require!(n > 0, "COS requires a nonzero expansion size");
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Ok(Self { base, model, l, n })
    }

    fn parameters(&self) -> HestonChf {
        let m = self.model.borrow();
        HestonChf::new(m.kappa(), m.theta(), m.sigma(), m.rho(), m.v0())
    }

    /// First cumulant of the normalized log return.
    pub fn c1(&self, t: Time) -> Real {
        hestoncumulants::c1(self.parameters(), t)
    }
    /// Second cumulant of the normalized log return.
    pub fn c2(&self, t: Time) -> Real {
        hestoncumulants::c2(self.parameters(), t)
    }
    /// Third cumulant of the normalized log return.
    pub fn c3(&self, t: Time) -> Real {
        hestoncumulants::c3(self.parameters(), t)
    }
    /// Fourth cumulant of the normalized log return.
    pub fn c4(&self, t: Time) -> Real {
        hestoncumulants::c4(self.parameters(), t)
    }
    /// Normalized log-return characteristic function at a real frequency.
    pub fn chf(&self, u: Real, t: Time) -> Complex {
        self.parameters().chf(Complex::new(u, 0.0), t)
    }
    /// Logarithm of the forward-to-spot ratio at the supplied time.
    pub fn mu_t(&self, t: Time) -> QlResult<Real> {
        let m = self.model.borrow();
        let p = m.process();
        Ok((p.dividend_yield().current_link()?.discount(t, false)?
            / p.risk_free_rate().current_link()?.discount(t, false)?)
        .ln())
    }
}

impl AsObservable for CosHestonEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl PricingEngine for CosHestonEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }
    fn results(&self) -> &dyn Results {
        self.base.results()
    }
    fn reset(&mut self) {
        self.base.reset();
    }
    fn calculate(&mut self) -> QlResult<()> {
        let m = HestonMarket::new(self.base.arguments(), &self.model.borrow())?;
        let x = (m.forward / m.strike).ln();
        let c1 = hestoncumulants::c1(m.chf, m.time);
        let w = hestoncumulants::c2(m.chf, m.time).abs().sqrt();
        let a = x + c1 - self.l * w;
        let b = x + c1 + self.l * w;
        require!(
            a.is_finite() && b.is_finite() && b > a,
            "invalid COS truncation interval"
        );
        let value = if x >= b / 2.0 || x <= a / 2.0 {
            match m.option_type {
                OptionType::Call => (m.forward - m.strike).max(0.0) * m.discount,
                OptionType::Put => (m.strike - m.forward).max(0.0) * m.discount,
            }
        } else {
            let d = 1.0 / (b - a);
            let exp_a = a.exp();
            let mut s = m.chf.chf(Complex::new(0.0, 0.0), m.time).re * (exp_a - 1.0 - a) * d;
            for n in 1..self.n {
                let r = n as Real * std::f64::consts::PI * d;
                let u = 2.0
                    * d
                    * ((exp_a + r * (r * a).sin() - (r * a).cos()) / (1.0 + r * r)
                        - (r * a).sin() / r);
                s += u
                    * (m.chf.chf(Complex::new(r, 0.0), m.time)
                        * Complex::new(0.0, r * (x - a)).exp())
                    .re;
            }
            match m.option_type {
                OptionType::Put => m.strike * m.discount * s,
                OptionType::Call => m.discount * (m.forward - m.strike * (1.0 - s)),
            }
        };
        require!(value.is_finite(), "nonfinite COS Heston price");
        self.base.results_mut().instrument.value = Some(value);
        Ok(())
    }
}
