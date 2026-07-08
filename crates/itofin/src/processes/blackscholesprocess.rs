//! Black-Scholes processes.
//!
//! Port of `ql/processes/blackscholesprocess.{hpp,cpp}`:
//! [`GeneralizedBlackScholesProcess`] describes the stochastic process
//! `d ln S(t) = (r(t) - q(t) - sigma(t, S)^2 / 2) dt + sigma dW_t`, built
//! from a spot quote handle plus risk-free, dividend and Black-volatility
//! term-structure handles. While the interface is expressed in terms of `S`,
//! the internal calculations work on `ln S`, so
//! [`apply`](crate::stochasticprocess::StochasticProcess1D::apply) composes
//! multiplicatively, exactly as in C++.
//!
//! The local volatility is derived lazily from the Black volatility and
//! cached until an input notification invalidates it (the C++ `update()`
//! resets `updated_` before notifying, mirrored here by the invalidating
//! observer). Only the `BlackConstantVol` shortcut of the C++ dispatch is
//! ported: it marks the process strike-independent and enables the exact
//! curve formulas for `expectation` / `variance` / `evolve`. The
//! `BlackVarianceCurve` shortcut and the strike-dependent `LocalVolSurface`
//! fallback follow with EPIC-4, so a non-constant Black volatility without an
//! external local volatility is an `Err` for now instead of silently wrong.
//!
//! Not ported, noted as follow-up: the pluggable `discretization` strategy
//! and `forceDiscretization` flag (the Euler scheme is the trait's provided
//! default, see [`crate::stochasticprocess`]), and the sibling conveniences
//! `BlackScholesProcess` (its dividend-free curve needs a D5 settings
//! decision), `BlackProcess` and `GarmanKohlagenProcess`.

use std::any::Any;
use std::cell::Cell;

use crate::errors::QlResult;
use crate::fail;
use crate::handle::{Handle, RelinkableHandle};
use crate::interestrate::Compounding;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::quotes::Quote;
use crate::require;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::stochasticprocess::StochasticProcess1D;
use crate::termstructures::TermStructure;
use crate::termstructures::volatility::{
    BlackConstantVol, BlackVolTermStructure, LocalConstantVol, LocalVolTermStructure,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// Observer half of the process (the C++
/// `GeneralizedBlackScholesProcess::update()`): drops the cached local
/// volatility, then forwards the notification to the process observers.
struct CacheInvalidator {
    updated: Shared<Cell<bool>>,
    observable: Shared<Observable>,
}

impl Observer for CacheInvalidator {
    fn update(&mut self) {
        self.updated.set(false);
        self.observable.notify_observers();
    }
}

/// Generalized Black-Scholes stochastic process.
pub struct GeneralizedBlackScholesProcess {
    x0: Handle<dyn Quote>,
    risk_free_rate: Handle<dyn YieldTermStructure>,
    dividend_yield: Handle<dyn YieldTermStructure>,
    black_volatility: Handle<dyn BlackVolTermStructure>,
    external_local_vol: Option<Handle<dyn LocalVolTermStructure>>,
    local_volatility: RelinkableHandle<dyn LocalVolTermStructure>,
    updated: Shared<Cell<bool>>,
    is_strike_independent: Cell<bool>,
    observable: Shared<Observable>,
    _listener: SharedMut<CacheInvalidator>,
}

/// Merton (1973) extension to the Black-Scholes stochastic process: a stock
/// or stock index paying a continuous dividend yield. Identical to the
/// generalized process, as in C++ where the subclass adds nothing.
pub type BlackScholesMertonProcess = GeneralizedBlackScholesProcess;

impl GeneralizedBlackScholesProcess {
    fn assemble(
        x0: Handle<dyn Quote>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        black_volatility: Handle<dyn BlackVolTermStructure>,
        external_local_vol: Option<Handle<dyn LocalVolTermStructure>>,
    ) -> GeneralizedBlackScholesProcess {
        let updated = shared(Cell::new(false));
        let observable = shared(Observable::new());
        let listener = shared_mut(CacheInvalidator {
            updated: Shared::clone(&updated),
            observable: Shared::clone(&observable),
        });
        let observer = listener.clone() as SharedMut<dyn Observer>;
        x0.register_observer(&observer);
        risk_free_rate.register_observer(&observer);
        dividend_yield.register_observer(&observer);
        black_volatility.register_observer(&observer);
        if let Some(local_vol) = &external_local_vol {
            local_vol.register_observer(&observer);
        }
        GeneralizedBlackScholesProcess {
            x0,
            risk_free_rate,
            dividend_yield,
            black_volatility,
            external_local_vol,
            local_volatility: RelinkableHandle::empty(),
            updated,
            is_strike_independent: Cell::new(false),
            observable,
            _listener: listener,
        }
    }

    /// Process deriving its local volatility from the Black volatility.
    pub fn new(
        x0: Handle<dyn Quote>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        black_volatility: Handle<dyn BlackVolTermStructure>,
    ) -> GeneralizedBlackScholesProcess {
        Self::assemble(x0, dividend_yield, risk_free_rate, black_volatility, None)
    }

    /// Process with an externally supplied local volatility.
    pub fn with_local_vol(
        x0: Handle<dyn Quote>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        black_volatility: Handle<dyn BlackVolTermStructure>,
        local_vol: Handle<dyn LocalVolTermStructure>,
    ) -> GeneralizedBlackScholesProcess {
        Self::assemble(
            x0,
            dividend_yield,
            risk_free_rate,
            black_volatility,
            Some(local_vol),
        )
    }

    /// The spot quote handle.
    pub fn state_variable(&self) -> Handle<dyn Quote> {
        self.x0.clone()
    }

    /// The dividend-yield curve handle.
    pub fn dividend_yield(&self) -> Handle<dyn YieldTermStructure> {
        self.dividend_yield.clone()
    }

    /// The risk-free-rate curve handle.
    pub fn risk_free_rate(&self) -> Handle<dyn YieldTermStructure> {
        self.risk_free_rate.clone()
    }

    /// The Black-volatility curve handle.
    pub fn black_volatility(&self) -> Handle<dyn BlackVolTermStructure> {
        self.black_volatility.clone()
    }

    /// The local-volatility curve, derived from the Black volatility and
    /// cached until an input changes (or the external one, when supplied).
    pub fn local_volatility(&self) -> QlResult<Handle<dyn LocalVolTermStructure>> {
        if let Some(external) = &self.external_local_vol {
            return Ok(external.clone());
        }
        if !self.updated.get() {
            self.is_strike_independent.set(true);
            let black_vol = self.black_volatility.current_link()?;
            let Some(constant_vol) = (&*black_vol as &dyn Any).downcast_ref::<BlackConstantVol>()
            else {
                self.is_strike_independent.set(false);
                fail!(
                    "only constant Black volatilities are supported; the local-volatility \
                     surface follows with EPIC-4"
                );
            };
            let reference_date = constant_vol.reference_date()?;
            let vol = constant_vol.black_vol(0.0, self.x0()?, false)?;
            let Some(day_counter) = constant_vol.day_counter() else {
                fail!("no day counter provided for the constant Black volatility");
            };
            self.local_volatility.link_to(shared(LocalConstantVol::new(
                reference_date,
                vol,
                day_counter,
            )) as Shared<dyn LocalVolTermStructure>);
            self.updated.set(true);
        }
        Ok(self.local_volatility.handle())
    }

    fn forward(
        &self,
        curve: &Handle<dyn YieldTermStructure>,
        t1: Time,
        t2: Time,
    ) -> QlResult<Rate> {
        Ok(curve
            .current_link()?
            .forward_rate(
                t1,
                t2,
                Compounding::Continuous,
                Frequency::NoFrequency,
                true,
            )?
            .rate())
    }

    fn exact_growth_rate(&self, t0: Time, dt: Time) -> QlResult<Rate> {
        let r = self.forward(&self.risk_free_rate, t0, t0 + dt)?;
        let q = self.forward(&self.dividend_yield, t0, t0 + dt)?;
        Ok(r - q)
    }
}

impl AsObservable for GeneralizedBlackScholesProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess1D for GeneralizedBlackScholesProcess {
    fn x0(&self) -> QlResult<Real> {
        self.x0.current_link()?.value()
    }

    fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
        let sigma = self.diffusion(t, x)?;
        let t1 = t + 0.0001;
        let r = self.forward(&self.risk_free_rate, t, t1)?;
        let q = self.forward(&self.dividend_yield, t, t1)?;
        Ok(r - q - 0.5 * sigma * sigma)
    }

    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
        self.local_volatility()?
            .current_link()?
            .local_vol(t, x, true)
    }

    fn expectation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        self.local_volatility()?;
        require!(self.is_strike_independent.get(), "not implemented");
        Ok(x0 * (dt * self.exact_growth_rate(t0, dt)?).exp())
    }

    fn std_deviation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        self.local_volatility()?;
        if self.is_strike_independent.get() {
            Ok(self.variance(t0, x0, dt)?.sqrt())
        } else {
            Ok(self.diffusion(t0, x0)? * dt.sqrt())
        }
    }

    fn variance(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        self.local_volatility()?;
        if self.is_strike_independent.get() {
            let vol = self.black_volatility.current_link()?;
            Ok(vol.black_variance(t0 + dt, 0.01, false)? - vol.black_variance(t0, 0.01, false)?)
        } else {
            let sigma = self.diffusion(t0, x0)?;
            Ok(sigma * sigma * dt)
        }
    }

    fn evolve(&self, t0: Time, x0: Real, dt: Time, dw: Real) -> QlResult<Real> {
        self.local_volatility()?;
        if self.is_strike_independent.get() {
            let var = self.variance(t0, x0, dt)?;
            let drift = self.exact_growth_rate(t0, dt)? * dt - 0.5 * var;
            Ok(self.apply(x0, var.sqrt() * dw + drift))
        } else {
            let drift = self.drift(t0, x0)? * dt;
            Ok(self.apply(x0, drift + self.std_deviation(t0, x0, dt)? * dw))
        }
    }

    fn apply(&self, x0: Real, dx: Real) -> Real {
        x0 * dx.exp()
    }

    fn time(&self, date: &Date) -> QlResult<Time> {
        self.risk_free_rate
            .current_link()?
            .time_from_reference(*date)
    }
}
